"""Build audited move/repair/commit demonstrations from recorded policy training."""
import argparse
from collections import Counter
from copy import deepcopy
import json
from pathlib import Path
import tempfile

import torch

from audit_blueprint_ppo import WitnessAudit
from blueprint_edit_input import prepare
from blueprint_ppo import check_sources
from blueprint_ppo_environment import Runtime, semantic
from blueprint_ppo_store import ROOT, checked, load, read, reference, save, sha, write
from blueprint_ppo_update import digest, require
from post_pose_recovery import advance, draft


def replay(runtime, saved, actions, checker):
    session = runtime.restore(saved)
    rows = []
    for action in actions:
        observation = json.loads(session.observation_json())
        prepared = prepare(observation)
        require(action in prepared['actions'], f'blueprint.bc-data: absent candidate {action}')
        outcome = runtime.step(session, observation, action)
        rows.append(dict(observation=observation, input=prepared, index=prepared['actions'].index(action),
                         action=action, outcome=outcome))
        if outcome['status'] == 'rejected':
            return dict(status='rejected', rows=rows, witness=None, score=None)
    require(rows[-1]['action']['kind'] == 'commit' and rows[-1]['outcome']['status'] in ('validated', 'improved'),
            'blueprint.bc-data: sequence ends in validated commit')
    after = json.loads(session.observation_json())
    witness = runtime.commit_witness(session, after)
    checker.check(after['blueprint'], witness)
    return dict(status='validated', rows=rows, witness=witness,
                score=rows[-1]['outcome']['details']['native_acceptance']['metrics']['score'])


def before_checkpoint(source, case):
    chunk = load(checked(case['source_chunk'], source))
    identity = case['identity']
    saved = next(s['native'] for s in chunk['native_before'] if s['index'] == identity['stream'])
    rows = [s for s in chunk['samples'] if s['stream'] == identity['stream']]
    for i, sample in enumerate(rows):
        if sample['policy_action'] == identity['policy_action']:
            require(semantic(sample['observation']) == semantic(case['before']), 'blueprint.bc-data: source pre-pose identity')
            return deepcopy(saved)
        saved = advance(saved, sample, rows[i + 1]['observation'])
    raise ValueError(f'blueprint.bc-data: source action missing: {identity}')


def build(config_path, output):
    require(not output.exists(), f'blueprint.bc-data: fresh output required: {output}')
    config = read(config_path)
    source, recovery = [ROOT / config['bc'][k] for k in ('source_run', 'recovery_run')]
    protocol = read(source / 'protocol.json')
    check_sources(protocol)
    source_audit = read(source / 'audit.json')
    require(source_audit['status'] == 'passed' and source_audit['protocol_sha256'] == sha(source / 'protocol.json'), 'blueprint.bc-data: audited source')
    progress = read(recovery / 'progress.json')
    census = read(checked(progress['report'], recovery))
    recovery_protocol = read(recovery / 'protocol.json')
    require(census['protocol_sha256'] == sha(recovery / 'protocol.json'), 'blueprint.bc-data: recovery protocol')
    require(recovery_protocol['source_protocol']['sha256'] == sha(source / 'protocol.json'), 'blueprint.bc-data: same source run')
    for key, file in [('source_state', 'state.json'), ('source_audit', 'audit.json')]:
        require(recovery_protocol[key]['sha256'] == sha(source / file), 'blueprint.bc-data: source census binding')
    runtime = Runtime(config, protocol['native_library'])
    candidates, excluded, demos = [], [], {}
    with tempfile.TemporaryDirectory(prefix='blueprint-bc-data-') as temporary:
        checker = WitnessAudit(runtime, temporary)
        for row in census['cases']:
            if not row['improves_area']:
                continue
            candidates.append(row['name'])
            if row['start'] in config['bc']['excluded_starts']:
                excluded.append(dict(case=row['name'], reason='supplied_compaction_start'))
                continue
            case = load(checked(row['case'], recovery))
            saved = before_checkpoint(source, case)
            actual = json.loads(runtime.restore(saved).observation_json())
            require(semantic(actual) == semantic(case['before']), 'blueprint.bc-data: restored pre-pose physical identity')
            finish = [dict(kind='repair_all'), dict(kind='commit')]
            before = replay(runtime, saved, finish, checker)
            after = replay(runtime, saved, [case['action'], *finish], checker)
            if before['status'] != 'validated' or after['status'] != 'validated':
                excluded.append(dict(case=row['name'], reason='inconclusive_branch', before=before['status'], after=after['status']))
                continue
            if after['score'][0] >= min(before['score'][0], actual['best_metrics']['score'][0]):
                excluded.append(dict(case=row['name'], reason='move_does_not_reduce_recoverable_area', before=before['score'], after=after['score']))
                continue
            group = digest(dict(draft=draft(actual), best=saved['best_draft']))
            identity = digest(dict(group=group, action=case['action']))
            provenance = dict(name=row['name'], identity=case['identity'], source_case=row['case'], source_chunk=case['source_chunk'])
            if identity in demos:
                demos[identity]['sources'].append(provenance)
                continue
            demos[identity] = dict(id=identity, group=group, sources=[provenance], checkpoint=saved,
                                  before=before, after=after, best_score=actual['best_metrics']['score'])
        groups = sorted({d['group'] for d in demos.values()})
        held = set(groups[::config['bc']['holdout_stride']])
        for demo in demos.values():
            demo['split'] = 'holdout' if demo['group'] in held else 'train'
        require(bool(held) and len(held) < len(groups), 'blueprint.bc-data: nonempty independent group splits')
        counts = dict(Counter(d['split'] for d in demos.values()))
        dataset = dict(schema='blueprint-bc-dataset-v1', config_sha256=sha(config_path),
            source_run=str(source), recovery_run=str(recovery),
            source_protocol=reference(source / 'protocol.json', source),
            source_audit=reference(source / 'audit.json', source),
            recovery_report=reference(recovery / 'report.json', recovery),
            build_source_sha256=sha(Path(__file__)), native_sha256=protocol['native_sha256'],
            demonstrations=list(demos.values()), candidates=candidates, excluded=excluded)
        output.mkdir(parents=True)
        save(output / 'dataset.pt.gz', dataset)
        summary = dict(schema='blueprint-bc-data-summary-v1', status='passed', dataset=reference(output / 'dataset.pt.gz', output),
            candidates=len(candidates), counts=counts, unique_groups=len(groups), holdout_groups=len(held),
            labels={k: v * 3 for k, v in counts.items()},
            accepted_source_cases=sum(len(d['sources']) for d in demos.values()),
            exclusion_reasons=dict(Counter(r['reason'] for r in excluded)),
            witnesses=len(checker.unique), witness_references=checker.references,
            examples=[dict(id=d['id'], split=d['split'], sources=[s['name'] for s in d['sources']],
                           before=d['before']['score'], after=d['after']['score']) for d in demos.values()])
        write(output / 'summary.json', summary)
        print(json.dumps(summary), flush=True)
    check_sources(protocol)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--config', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    torch.set_num_threads(1)
    build(args.config.resolve(), args.output.resolve())


if __name__ == '__main__':
    main()
