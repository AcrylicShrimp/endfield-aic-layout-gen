"""Explicit retained-trace diagnosis; only unobserved commit opportunities are probed."""
import argparse
from collections import Counter
from concurrent.futures import ProcessPoolExecutor, as_completed
import gzip
import hashlib
import io
import json
import multiprocessing as mp
import os
from pathlib import Path
import sys
import time


def check(ok, code, message, entity=None):
    if not ok:
        raise ValueError(json.dumps(dict(code=f'commit-diagnosis.{code}', severity='error',
            stage='retained-commit', message=message, entity=str(entity))))


def read(path):
    return json.loads(Path(path).read_text())


def sha(data):
    return hashlib.sha256(data).hexdigest()


def write(path, value):
    Path(path).write_text(json.dumps(value, indent=2) + '\n')


def authenticated(path, expected):
    data = Path(path).read_bytes()
    check(sha(data) == expected, 'source-invalid', 'Selected evidence hash differs', path)
    return data


def draft_key(observation):
    return sha(json.dumps(dict(blueprint=observation['blueprint_sha256'],
        facilities=observation['facilities'], paths=[e['path'] for e in observation['edges']]),
        sort_keys=True, separators=(',', ':')).encode())


def footprint_floor(observation):
    """Facility bodies are a subset of the used geometry; no feasibility claim."""
    rows = observation['facilities']
    check(bool(rows), 'state-invalid', 'Expected a nonempty factory')
    return ((max(f['x'] + f['width'] for f in rows) - min(f['x'] for f in rows)) *
            (max(f['y'] + f['height'] for f in rows) - min(f['y'] for f in rows)))


def classify(improved, opportunity, skipped):
    if improved:
        return 'improved_with_skipped_opportunity' if skipped else 'improved'
    return 'failed_with_useful_commit_available' if opportunity else 'failed_without_useful_commit_available'


def initialize(root, library, catalogs, deadline, counter, limit, cores, worker_index):
    global ROOT, MODULE, CATALOGS, DEADLINE, PROBES, LIMIT, CORE
    import torch
    from native import load_native
    torch.set_num_threads(1)
    with worker_index.get_lock():
        index = worker_index.value
        worker_index.value += 1
    CORE = cores[index % len(cores)]
    os.sched_setaffinity(0, {CORE})
    ROOT, MODULE, CATALOGS = root, load_native(library), catalogs
    DEADLINE, PROBES, LIMIT = deadline, counter, limit


def guard():
    check(time.monotonic() < DEADLINE, 'resource-exhausted', 'Diagnostic wall ceiling reached')


def load_trace(path, expected):
    import torch
    guard()
    data = authenticated(path, expected)
    with io.BytesIO(gzip.decompress(data)) as stream:
        return torch.load(stream, map_location='cpu', weights_only=False)


def native_score(template, observation, best_draft):
    from blueprint_ppo_environment import Runtime
    from post_pose_recovery import draft, restore_verified
    with PROBES.get_lock():
        check(PROBES.value < LIMIT, 'resource-exhausted', 'Unique native probe ceiling reached')
        PROBES.value += 1
    saved = dict(template, draft=draft(observation), best_draft=best_draft,
        attempts=observation['attempts'], revision=observation['revision'], closed=False,
        draft_validated=observation['draft_factory_validated'],
        elapsed_ns=round(observation['elapsed_ms'] * 1_000_000))
    runtime = object.__new__(Runtime)
    runtime.module, runtime.spec = MODULE, dict(catalogs=CATALOGS)
    runtime.original, runtime.request = saved['original'], saved['request']
    session = restore_verified(runtime, saved, observation)
    outcome = runtime.step(session, observation, dict(kind='commit'))
    codes = [d['code'] for d in outcome['diagnostics']]
    check(not any(c.endswith(('budget-exhausted', 'elapsed')) for c in codes),
        'resource-exhausted', 'Native commit could not use the original remaining allowance', codes)
    accepted = outcome.get('details', {}).get('native_acceptance')
    valid = outcome['status'] in ('improved', 'validated')
    if valid:
        check(accepted and accepted['factory_validated'], 'state-invalid', 'Commit lacks native acceptance')
        runtime.commit_witness(session, json.loads(session.observation_json()))
    return dict(valid=valid, score=accepted['metrics']['score'] if valid else None,
        diagnostics=codes, evidence='one-native-commit', restored_attempts=observation['attempts'])


def analyze_case(task):
    from post_pose_recovery import draft
    began = time.monotonic()
    request = json.loads(authenticated(task['request']['path'], task['request']['sha256']))
    original = json.loads(authenticated(task['initial']['path'], task['initial']['sha256']))
    cache, outputs, probes = {}, [], []
    template = None
    for record in task['traces']:
        payload = load_trace(record['path'], record['sha256'])
        trace, initial = payload['trace'], payload['initial']
        check(len(trace) == 64 and payload['result']['factory_validated'],
            'source-invalid', 'Expected complete 64-action valid-best evaluation', record['path'])
        if template is None:
            session = MODULE.BlueprintEditSession.open(ROOT, json.dumps(CATALOGS), json.dumps(request),
                json.dumps(original), json.dumps(initial['config']))
            template = json.loads(session.checkpoint_json())
        check(initial['blueprint_sha256'] == template['blueprint_sha256'],
            'source-invalid', 'Panel input and recorded blueprint differ', record['path'])
        # Recorded successful commits are already full physical witnesses.
        for entry in trace:
            if entry['action']['kind'] == 'commit' and entry['outcome']['status'] in ('validated', 'improved'):
                accepted = entry['outcome']['details']['native_acceptance']
                check(accepted['factory_validated'] and entry['validated_witness'] is not None,
                    'source-invalid', 'Recorded commit witness missing', record['path'])
                key = draft_key(entry['observation'])
                value = dict(valid=True, score=accepted['metrics']['score'], evidence='recorded-commit')
                if key in cache:
                    check(cache[key]['valid'] and cache[key]['score'] == value['score'],
                        'state-invalid', 'Identical physical draft has conflicting validation')
                cache[key] = value
        best_draft = draft(initial)
        counts, codes, opportunities = Counter(), Counter(), []
        in_window = False
        for index, entry in enumerate(trace):
            guard()
            observation, action, outcome = entry['observation'], entry['action'], entry['outcome']
            check(not observation['done'] and observation['attempts'] < observation['config']['max_actions'],
                'state-invalid', 'Pre-action observation has no remaining action', record['path'])
            counts['actions'] += 1
            counts[f"action/{action['kind']}"] += 1
            if action['kind'] == 'commit':
                counts['commit_attempts'] += 1
                counts[f"commit/{outcome['status']}"] += 1
                counts['positive_area_commits'] += outcome['reward'] > 0
                counts['valid_nonarea_commits'] += outcome['status'] in ('validated', 'improved') and outcome['reward'] <= 0
                codes.update(d['code'] for d in outcome['diagnostics'])
            pending = sum(e['status'] == 'pending' for e in observation['edges'])
            conflicts = bool(observation['conflicts'])
            counts['before_pending'] += pending > 0
            counts['before_conflicts'] += conflicts
            ready = not pending and not conflicts
            useful = False
            if ready:
                counts['surface_complete'] += 1
                best_area = observation['best_metrics']['score'][0]
                if footprint_floor(observation) >= best_area:
                    counts['area_excluded_by_body_floor'] += 1
                else:
                    key = draft_key(observation)
                    if key not in cache:
                        cache[key] = native_score(template, observation, best_draft)
                        probes.append(dict(key=key, source=record['path'], action=index + 1, **cache[key]))
                    candidate = cache[key]
                    counts['surface_physically_valid'] += candidate['valid']
                    counts['surface_physically_invalid'] += not candidate['valid']
                    useful = candidate['valid'] and candidate['score'][0] < best_area
                    if useful:
                        if not in_window:
                            counts['useful_windows'] += 1
                        counts['useful_observations'] += 1
                        skipped = action['kind'] != 'commit'
                        counts['skipped_useful_observations'] += skipped
                        counts[f"useful_action/{action['kind']}"] += 1
                        opportunities.append(dict(action=index + 1, selected=action,
                            key=key, draft_score=candidate['score'], current_best=observation['best_metrics']['score'],
                            skipped=skipped, selected_status=outcome['status'], evidence=candidate['evidence']))
            if in_window and not useful:
                previous = trace[index - 1]
                counts[f"window_exit/{previous['action']['kind']}/{previous['outcome']['status']}"] += 1
            in_window = useful
            if outcome['status'] == 'improved':
                check(action['kind'] == 'commit', 'state-invalid', 'Only commit can improve best')
                best_draft = draft(observation)
        final_area = payload['result']['best_metrics']['score'][0]
        improved = final_area < initial['best_metrics']['score'][0]
        best_available = min([o['draft_score'][0] for o in opportunities] + [final_area])
        counts['ever_opportunity'] = bool(opportunities)
        counts['ever_skipped_opportunity'] = any(o['skipped'] for o in opportunities)
        counts['improved_episode'] = improved
        counts['unclaimed_area_episode'] = best_available < final_area
        counts['failed_episode'] = not improved
        counts['failed_with_useful_commit_available'] = not improved and bool(opportunities)
        counts['failed_without_useful_commit_available'] = not improved and not opportunities
        outputs.append(dict(arm=record['arm'], seed=record['seed'], sample=record['sample'],
            generation_seed=task['generation_seed'], eligible=task['eligible'], source=record,
            classification=classify(improved, bool(opportunities), counts['ever_skipped_opportunity']),
            counters=dict(counts), rejection_codes=dict(codes), opportunities=opportunities,
            initial_area=initial['best_metrics']['score'][0], final_area=final_area,
            best_available_area=best_available, final_pending=trace[-1]['pending'], final_conflicts=trace[-1]['conflicts']))
    result = dict(case=task['generation_seed'], episodes=outputs, probes=probes,
        worker=dict(pid=os.getpid(), core=CORE), seconds=time.monotonic() - began)
    write(task['output'], result)
    return dict(path=task['output'], sha256=sha(Path(task['output']).read_bytes()),
        episodes=len(outputs), probes=len(probes), worker=result['worker'])


def anchor_summary(provenance, launch, output):
    manifest_path = Path(launch['source_directory']) / 'manifest.json'
    manifest = json.loads(authenticated(manifest_path, launch['bindings']['17']['manifest_sha256']))
    imports = manifest['imports']
    source = manifest_path.parent / imports['path']
    raw = authenticated(source, imports['sha256'])
    import torch
    sequences = torch.load(io.BytesIO(gzip.decompress(raw)), map_location='cpu', weights_only=False)['sequences']
    proofs = []
    for sequence in sequences:
        state = next(s for s in sequence['anchor_states'] if s['stage'] == 'commit')
        native, observation = state['native'], state['observation']
        proof = native_score(native, observation, native['best_draft'])
        check(proof['valid'] and proof['score'][0] == state['demonstrated_score'][0]
            and proof['score'][0] < observation['best_metrics']['score'][0],
            'state-invalid', 'Known pre-commit state did not yield demonstrated improvement')
        proofs.append(proof)
    check(len(proofs) == 5, 'source-invalid', 'Expected five authenticated anchor states')
    rows = []
    for name, reference in provenance['probe_records'].items():
        if not name.startswith('B50-') or not name.endswith('-update-128'):
            continue
        path = Path(reference['path'])
        report = json.loads(authenticated(path, reference['sha256']))
        for row in report['rows']:
            if row['stage'] != 'commit':
                continue
            trace_path = path.parent / f"case-{row['case_index']}-commit-{row['replicate']}.pt.gz"
            # Branch reports historically did not bind trace hashes; record them now
            # and verify their complete row against the previously bound report.
            raw = trace_path.read_bytes()
            import torch
            payload = torch.load(io.BytesIO(gzip.decompress(raw)), map_location='cpu', weights_only=False)
            check(payload['row'] == row, 'source-invalid', 'Anchor trace differs from bound branch row', trace_path)
            first = payload['trace'][0]
            rows.append(dict(seed=report['seed'], case=row['case_index'], sample=row['replicate'],
                improved=row['improved'], target=row['demonstrated_target_attained'],
                first_action=first['action']['kind'], first_status=first['outcome']['status'],
                first_reward=first['outcome']['reward'], final_pending=row['pending'], final_conflicts=row['conflicts'],
                commit_attempts=sum(t['action']['kind'] == 'commit' for t in payload['trace']),
                source=dict(path=str(trace_path), sha256=sha(raw)),
                historical_trace_hash_bound=False))
    check(len(rows) == 120, 'source-invalid', 'Expected 120 known B50 pre-commit branches')
    write(output / 'anchors.json', dict(rows=rows, initial_commit_proofs=proofs,
        admitted_imports=dict(path=str(source), sha256=imports['sha256'])))
    return rows


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--matrix', type=Path, required=True)
    parser.add_argument('--provenance', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--workers', type=int, default=8)
    parser.add_argument('--seconds', type=int, default=600)
    parser.add_argument('--max-probes', type=int, default=2048)
    args = parser.parse_args()
    check(not args.output.exists(), 'output-exists', 'Fresh diagnostic output required', args.output)
    check(1 <= args.workers <= 8 and args.seconds > 0 and args.max_probes > 0,
        'config-invalid', 'Expected 1..8 workers and positive limits')
    args.output.mkdir(parents=True)
    began = time.monotonic()
    provenance = read(args.provenance)
    matrix = args.matrix.resolve()
    root = matrix.parents[2]
    sys.path.insert(0, str(root / 'tools/factory-edit-pilot'))
    import torch
    from blueprint_ppo_environment import semantic
    from post_pose_recovery import draft, restore_verified
    launch = json.loads(authenticated(matrix / 'launch.json', provenance['launch']['sha256']))
    state = json.loads(authenticated(matrix / 'state.json', provenance['matrix_state']['sha256']))
    check(state['status'] == 'complete', 'source-invalid', 'Matrix must already be complete')
    analysis = json.loads(authenticated(provenance['analysis']['path'], provenance['analysis']['sha256']))
    check(analysis['status'] == 'complete', 'source-invalid', 'Expected completed registered report')
    bindings = {}
    for module in list(sys.modules.values()):
        path = getattr(module, '__file__', None)
        if path and Path(path).resolve().is_relative_to(root / 'tools/factory-edit-pilot'):
            relative = str(Path(path).resolve().relative_to(root))
            authenticated(path, launch['sources'][relative])
            bindings[relative] = launch['sources'][relative]
    library = launch['native_library']
    authenticated(library, launch['native_sha256'])
    probe_path = root / 'tools/factory-edit-pilot/blueprint-edit-probe.json'
    probe = json.loads(authenticated(probe_path, launch['sources'][str(probe_path.relative_to(root))]))
    for path in probe['catalogs'].values():
        path = root / path
        expected = launch['sources'].get(str(path.relative_to(root)), launch['external_sources'].get(str(path)))
        check(expected is not None, 'source-invalid', 'Catalog missing launch binding', path)
        authenticated(path, expected)
    panel_path = matrix / launch['transfer_panel']['path']
    panel = json.loads(authenticated(panel_path, launch['transfer_panel']['sha256']))
    eligible = {r['generation_seed']: r['eligible'] for r in analysis['eligibility']}
    tasks = []
    for case in panel['cases']:
        files = {k:dict(v, path=str(Path(case['directory']) / v['path'])) for k,v in case['files'].items()}
        tasks.append(dict(generation_seed=case['generation_seed'], eligible=eligible[case['generation_seed']],
            request=files['request'], initial=files['initial'], traces=[],
            output=str(args.output.resolve() / f"case-{case['generation_seed']}.json")))
    lookup = {t['generation_seed']: t for t in tasks}
    for name, binding in provenance['jobs'].items():
        directory = matrix / name
        job = json.loads(authenticated(directory / 'state.json', binding['state']['sha256']))
        ref = next(r for r in job['transfer'] if r['update'] == 128)
        report = json.loads(authenticated(directory / ref['path'], ref['sha256']))
        refs = {Path(r['path']).name:r for r in report['cases']}
        for row in report['rows']:
            file = refs[f"factory-{row['generation_seed']}-{row['sample']}.pt.gz"]
            lookup[row['generation_seed']]['traces'].append(dict(path=str(directory / file['path']),
                sha256=file['sha256'], arm=name.split('-seed-')[0], seed=row['seed'], sample=row['sample']))
    check(sum(len(t['traces']) for t in tasks) == 480, 'source-invalid', 'Expected 480 endpoint transfer traces')
    context = mp.get_context('fork')
    counter, worker_index = context.Value('i', 0), context.Value('i', 0)
    cores = sorted(os.sched_getaffinity(0))
    check(len(cores) >= args.workers, 'resource-exhausted', 'Distinct CPU cores unavailable')
    records = []
    try:
        with ProcessPoolExecutor(max_workers=args.workers, mp_context=context, initializer=initialize,
                initargs=(str(root), library, probe['catalogs'], began + args.seconds,
                    counter, args.max_probes, cores, worker_index)) as pool:
            futures = [pool.submit(analyze_case, task) for task in tasks]
            for future in as_completed(futures, timeout=max(1, args.seconds - (time.monotonic() - began))):
                record = future.result()
                records.append(record)
                write(args.output / 'progress.json', dict(records=records))
                print(json.dumps(dict(completed_cases=len(records), native_probes=counter.value)), flush=True)
        # Five isolated native commit proofs authenticate known pre-commit starts.
        # Workers are already closed; no models, training or policy rollouts run.
        from native import load_native
        globals().update(ROOT=str(root), MODULE=load_native(library), CATALOGS=probe['catalogs'],
            PROBES=counter, LIMIT=args.max_probes, DEADLINE=began + args.seconds)
        guard()
        anchors = anchor_summary(provenance, launch, args.output)
        episodes = [r for ref in records for r in read(ref['path'])['episodes']]
        groups = {}
        for population in ('strict', 'all16'):
            for name in provenance['jobs']:
                arm, seed = name.split('-seed-')
                selected = [r for r in episodes if r['arm'] == arm and r['seed'] == int(seed)
                    and (r['eligible'] or population == 'all16')]
                counts, rejections = Counter(), Counter()
                for row in selected:
                    counts.update(row['counters'])
                    rejections.update(row['rejection_codes'])
                groups[f'{population}/{name}'] = dict(episodes=len(selected), counters=dict(counts),
                    rejection_codes=dict(rejections), classifications=dict(Counter(r['classification'] for r in selected)))
        result = dict(schema='blueprint-commit-diagnosis-v1', status='complete',
            scope=dict(transfer_episodes=480, strict_episodes=sum(r['eligible'] for r in episodes),
                anchor_branches=len(anchors), new_policy_actions=0, optimizer_steps=0),
            source_sha256=sha(Path(__file__).read_bytes()), runtime_sources=bindings,
            provenance=dict(path=str(args.provenance.resolve()), sha256=sha(args.provenance.read_bytes())),
            native_sha256=launch['native_sha256'], options=vars(args) | dict(matrix=str(matrix),
                provenance=str(args.provenance.resolve()), output=str(args.output.resolve())),
            seconds=time.monotonic() - began, native_probes=counter.value, records=records, groups=groups,
            anchors={str(s):dict(branches=sum(r['seed']==s for r in anchors),
                improved=sum(r['seed']==s and r['improved'] for r in anchors),
                targets=sum(r['seed']==s and r['target'] for r in anchors),
                first_commit=sum(r['seed']==s and r['first_action']=='commit' for r in anchors),
                failed_first_actions=dict(Counter(r['first_action'] for r in anchors if r['seed']==s and not r['improved'])))
                for s in launch['spec']['seeds']})
        write(args.output / 'report.json', result)
        print(json.dumps(dict(status='complete', seconds=result['seconds'], native_probes=counter.value)))
    except BaseException as error:
        write(args.output / 'failure.json', dict(status='incomplete', records=records,
            diagnostic=dict(code='commit-diagnosis.failed', severity='error', stage='retained-commit',
                message=str(error), entity=str(args.output))))
        raise


if __name__ == '__main__':
    try:
        main()
    except Exception as error:
        print(json.dumps(dict(code='commit-diagnosis.failed', severity='error',
            stage='retained-commit', message=str(error), entity='command')), file=sys.stderr)
        raise SystemExit(1)
