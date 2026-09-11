"""Census actual policy poses, probe recovery and classify their continuations."""
import argparse
from collections import Counter, defaultdict
import json
from pathlib import Path
import tempfile
import time

import torch

from audit_blueprint_ppo import WitnessAudit
from blueprint_ppo import check_sources
from blueprint_ppo_environment import Runtime, semantic
from blueprint_ppo_store import ROOT, checked, load, read, reference, save, sha, write
from blueprint_ppo_update import require
from post_pose_recovery import advance, geometry, probe


def continuation(case, episode):
    future = [s for s in episode if s['policy_action'] > case['policy_action']]
    counts, boundary, steps = Counter(), None, 0
    for row in future:
        kind = row['action']['kind']
        steps += 1
        if kind in ('repair', 'repair_all'):
            counts['repair_actions'] += 1
            counts['repair_actions_with_pending'] += bool(row['pending_before'])
            counts['repair_actions_reducing_pending'] += len(row['pending_after']) < len(row['pending_before'])
            counts['repair_rejections'] += row['status'] == 'rejected'
            if kind == 'repair':
                counts['individual_repairs_of_routed_edges'] += row['action']['edge'] not in row['pending_before']
        if kind == 'restore_best' and row['status'] != 'rejected':
            boundary = 'restore_best'
        elif kind == 'pose' and row['status'] != 'rejected':
            boundary = 'next_pose'
        elif kind == 'commit' and row['status'] in ('validated', 'improved'):
            boundary = 'valid_commit'
        elif row['terminal']:
            boundary = 'terminal'
        if boundary:
            break
    counts['steps_until_boundary'] = steps
    if boundary is None:
        boundary = 'terminal'
    successful_commits = [s for s in future if s['action']['kind'] == 'commit' and s['status'] in ('validated', 'improved')]
    first_restore = next((s['policy_action'] for s in future if s['action']['kind'] == 'restore_best' and s['status'] != 'rejected'), None)
    before_restore = [s for s in future if first_restore is None or s['policy_action'] < first_restore]
    conflicts_cleared = next((s for s in before_restore if not s['conflicts_after']), None)
    return dict(next_boundary=boundary, counts=dict(counts),
        boundary_action=row if future and boundary else None,
        later_valid_commit=bool(successful_commits),
        later_area_improvement=any(s['reward'] > 0 for s in future),
        first_restore_action=first_restore,
        conflicts_cleared_before_restore=conflicts_cleared['policy_action'] if conflicts_cleared else None)


def compact(sample, successor):
    observation = sample['observation']
    return dict(stream=sample['stream'], episode=sample['episode'], policy_action=sample['policy_action'],
        action=sample['input']['actions'][sample['index']], status=sample['outcome']['status'],
        reward=sample['reward'], terminal=sample['terminal'],
        pending_before=[e['id'] for e in observation['edges'] if e['status'] == 'pending'],
        pending_after=[e['id'] for e in successor['edges'] if e['status'] == 'pending'],
        conflicts_before=observation['conflicts'], conflicts_after=successor['conflicts'],
        best_score=successor['best_metrics']['score'], diagnostics=sample['outcome']['diagnostics'],
        commit_score=sample['outcome']['details'].get('native_acceptance', {}).get('metrics', {}).get('score')
        if sample['outcome']['status'] in ('validated', 'improved') else None)


def run(source, config_path, output):
    require(not output.exists(), f'fresh diagnostic output required: {output}')
    source_protocol, state, audit = [read(source / p) for p in ('protocol.json', 'state.json', 'audit.json')]
    require(state['status'] == 'complete' and audit['status'] == 'passed', 'completed audited source run')
    require(state['protocol_sha256'] == audit['protocol_sha256'] == sha(source / 'protocol.json'), 'source protocol identity')
    check_sources(source_protocol)
    config = read(config_path)
    require(config['schema'] == 'blueprint-post-pose-recovery-v1', 'post-pose diagnostic schema')
    output.mkdir(parents=True)
    protocol = dict(schema='blueprint-post-pose-protocol-v1', config=config, source_run=str(source),
        source_protocol=reference(source / 'protocol.json', source), source_state=reference(source / 'state.json', source),
        source_audit=reference(source / 'audit.json', source), native_sha256=source_protocol['native_sha256'],
        diagnostic_sources={str(p.relative_to(ROOT)): sha(p) for p in (Path(__file__).resolve(),
            Path(__file__).with_name('post_pose_recovery.py').resolve(), config_path)})
    write(output / 'protocol.json', protocol)
    runtime = Runtime(source_protocol['config'], source_protocol['native_library'])
    began = time.perf_counter()
    def guard():
        require(time.perf_counter() - began < config['run_seconds'], 'post-pose diagnostic elapsed guard')
    cases, episodes, rejected = [], defaultdict(list), []
    try:
        with tempfile.TemporaryDirectory(prefix='post-pose-admission-') as temporary:
            checker = WitnessAudit(runtime, temporary)
            for seed in source_protocol['config']['seeds']:
                for ordinal, ref in enumerate(state['jobs'][str(seed)]['records'], 1):
                    guard()
                    record = read(checked(ref, source))
                    chunk = load(checked(record['chunk'], source))
                    require(chunk['protocol_sha256'] == state['protocol_sha256'], 'authenticated chunk protocol')
                    frontiers = {f['stream']: f['successor_observation'] for f in chunk['frontiers']}
                    for initial in chunk['native_before']:
                        saved = initial['native']
                        rows = [s for s in chunk['samples'] if s['stream'] == initial['index']]
                        for index, sample in enumerate(rows):
                            successor = rows[index + 1]['observation'] if index + 1 < len(rows) else frontiers[initial['index']]
                            saved = advance(saved, sample, successor)
                            row = compact(sample, successor)
                            row['update'] = ordinal
                            row['source_chunk'] = record['chunk']
                            episodes[seed, row['stream'], row['episode']].append(row)
                            if row['action']['kind'] != 'pose':
                                continue
                            identity = dict(seed=seed, update=ordinal, stream=row['stream'], episode=row['episode'],
                                            policy_action=row['policy_action'], start=sample['start'])
                            if row['status'] == 'rejected':
                                rejected.append(dict(**identity, action=row['action'], diagnostics=row['diagnostics']))
                                continue
                            result = probe(runtime, saved, successor, config, checker, guard)
                            name = f's{seed}-u{ordinal:04d}-v{row["stream"]}-a{row["policy_action"]:03d}'
                            path = output / 'cases' / f'{name}.pt.gz'
                            save(path, dict(schema='blueprint-post-pose-case-v1', identity=identity,
                                source_chunk=record['chunk'], source_semantic=sample['after_semantic'],
                                action=row['action'], before=sample['observation'], after=successor,
                                checkpoint=saved, recovery=result))
                            cases.append(dict(**identity, name=name, case=reference(path, output),
                                action=row['action'], before_pending=len(row['pending_before']),
                                after_pending=len(row['pending_after']), before_conflicts=len(row['conflicts_before']),
                                after_conflicts=len(row['conflicts_after']), start_score=successor['best_metrics']['score'],
                                classification=result['classification'], recovered_by=result.get('recovered_by'),
                                recovered_score=result.get('score'), improves_score=result.get('improves_score', False),
                                improves_area=result.get('improves_area', False),
                                branch_count=len(result['branches']),
                                expansions=sum(b['expansions'] for b in result['branches'])))
                    if ordinal % 8 == 0:
                        progress = dict(status='running', seed=seed, update=ordinal, cases=len(cases),
                                        classifications=dict(Counter(c['classification'] for c in cases)))
                        write(output / 'progress.json', progress)
                        print(json.dumps(progress), flush=True)
            for rows in episodes.values():
                require([s['policy_action'] for s in rows] == list(range(1, source_protocol['config']['training']['episode_actions'] + 1)), 'complete chronological source episode')
            for case in cases:
                case['continuation'] = continuation(case, episodes[case['seed'], case['stream'], case['episode']])
            require(len(cases) + len(rejected) == audit['counts']['kind/pose'], 'census covers every sampled training pose')
            report = dict(schema='blueprint-post-pose-report-v1', status='complete', protocol_sha256=sha(output / 'protocol.json'),
                source_pose_actions=len(cases) + len(rejected), accepted_poses=len(cases), rejected_poses=rejected,
                native_restorations=len(cases), physically_validated_unique_witnesses=len(checker.unique),
                witness_references=checker.references, seconds=time.perf_counter() - began, cases=cases,
                classifications=dict(Counter(c['classification'] for c in cases)),
                recoveries=dict(Counter(c['recovered_by'] for c in cases if c['recovered_by'])),
                next_boundaries=dict(Counter(c['continuation']['next_boundary'] for c in cases)),
                recovered_score_improvements=sum(c['improves_score'] for c in cases),
                recovered_area_improvements=sum(c['improves_area'] for c in cases))
            save(output / 'source-episodes.pt.gz', dict(episodes=dict(episodes)))
            report['source_episodes'] = reference(output / 'source-episodes.pt.gz', output)
            write(output / 'report.json', report)
            for path, expected in protocol['diagnostic_sources'].items():
                require(sha(ROOT / path) == expected, 'diagnostic source stayed frozen')
            check_sources(source_protocol)
            write(output / 'progress.json', dict(status='complete', report=reference(output / 'report.json', output)))
            print(json.dumps({k: v for k, v in report.items() if k not in ('cases', 'rejected_poses')}), flush=True)
    except BaseException as error:
        save(output / 'interrupted.pt.gz', dict(cases=cases, episodes=dict(episodes), rejected=rejected))
        write(output / 'progress.json', dict(status='failed', error=str(error)))
        raise


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run', type=Path, required=True)
    parser.add_argument('--config', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    torch.set_num_threads(1)
    run(args.run.resolve(), args.config.resolve(), args.output.resolve())


if __name__ == '__main__':
    main()
