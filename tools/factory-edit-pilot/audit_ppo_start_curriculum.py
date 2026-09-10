"""Check committed curriculum ancestry and immutable inputs without replaying learning."""
import argparse
import gzip
import json
from pathlib import Path

from full_factory_environment import digest
from ppo_harness_state import read, write, require, checked_path, load_state, check_sources
from ppo_start_curriculum import SCHEMA, choose


def check_records(root, protocol, records):
    by_batch = {}
    inherited, depth_counts = 0, {0: 0, 1: 0, 2: 0}
    for record in records:
        training = record['phase']['kind'] == 'training'
        batch = record['phase']['batch']
        for result in record['results']:
            task = read(Path(result['result_path']).with_name('task.json'))
            require(result['policy_sha256'] == record['behavior_checkpoint']['policy_sha256'] and
                    result['critic_sha256'] == record['behavior_checkpoint']['critic_sha256'],
                    'curriculum-audit-policy', 'result must use current batch actor and critic', 'audit', task['identity'])
            if not training:
                require('initial_start' not in task and 'start_curriculum' not in result['generation'],
                        'curriculum-audit-evaluation', 'evaluation must not inherit training factories', 'audit', task['identity'])
                continue
            start = result['generation']['start_curriculum']
            depth = start['depth']
            depth_counts[depth] += 1
            require(all(start[key] == value for key, value in task['initial_start'].items()),
                    'curriculum-audit-task', 'result ancestry differs from immutable task', 'audit', task['identity'])
            if result['status'] != 'complete':
                require(depth == 0, 'curriculum-audit-admission', 'inherited valid input was rejected', 'audit', task['identity'])
                continue
            row = result['row']
            if depth == 0:
                require(start['root']['batch'] == batch and start['root']['generation_seed'] == task['generation_seed'] and
                        start['root']['initial_sha256'] == result['generation']['initial_sha256'],
                        'curriculum-audit-root', 'fresh root identity differs', 'audit', task['identity'])
                continue
            parent = by_batch[batch - 1][start['parent']['index']]
            parent_start = parent['generation']['start_curriculum']
            require(parent_start['depth'] == depth - 1 and parent_start['root'] == start['root'] and
                    start['root']['batch'] == batch - depth and
                    start['best_sha256'] == parent['row']['best_sha256'] and
                    start['expected_metrics'] == row['initial_metrics'] == parent['row']['best_metrics'],
                    'curriculum-audit-lineage', 'start must equal the consecutive parent best', 'audit', task['identity'])
            require(read(checked_path(root, start['parent']['result']))['row'] == parent['row'],
                    'curriculum-audit-parent', 'parent result reference differs', 'audit', task['identity'])
            witness = json.loads(gzip.decompress(checked_path(root, start['files']['best']).read_bytes()))
            require(digest(witness) == start['best_sha256'], 'curriculum-audit-geometry',
                    'retained parent geometry differs', 'audit', task['identity'])
            for key in ('request', 'original'):
                path = checked_path(root, start['files'][key])
                expected = parent['generation']['case']['files_sha256'][Path(parent['generation']['case'][key]).name]
                require(start['files'][key]['sha256'] == expected, 'curriculum-audit-obligations',
                        'retained request obligations differ from parent', 'audit', path)
            # Readiness retains all inputs; long-run audits can use preserved
            # bank hashes after ordinary per-episode output cleanup.
            initial = Path(result['generation']['case']['initial'])
            if initial.exists():
                require(digest(read(initial)) == start['best_sha256'], 'curriculum-audit-initial',
                        'materialized input differs from parent best', 'audit', initial)
            inherited += 1
        if training:
            by_batch[batch] = {r['identity']['index']: r for r in record['results']}
            actual = read(checked_path(root, record['next_training_starts']))
            expected = choose(protocol['config']['start_curriculum'], record['job'], batch + 1,
                              protocol['config']['training']['attempts'], record['results'])
            require(actual['schema'] == SCHEMA and len(actual['slots']) == len(expected['slots']) and all(actual[k] == expected[k]
                    for k in ('job', 'batch', 'requested_counts', 'actual_counts', 'diagnostics')),
                    'curriculum-audit-plan', 'saved plan differs from deterministic allocation', 'audit', batch)
            for saved, planned in zip(actual['slots'], expected['slots']):
                require(all(saved[k] == v for k, v in planned.items() if k != 'donor'),
                        'curriculum-audit-plan', 'saved slot differs from deterministic allocation', 'audit', batch)
                if 'donor' in planned:
                    require(saved['parent']['index'] == planned['donor']['identity']['index'],
                            'curriculum-audit-donor', 'saved donor differs from deterministic selection', 'audit', batch)
                for ref in saved.get('files', {}).values():
                    checked_path(root, ref)
    return dict(training_batches=len(by_batch), depth_counts=depth_counts, inherited_inputs=inherited,
                consecutive_ancestry=True, current_batch_policy=True, deterministic_start_plans=True,
                evaluation_not_inherited=True)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    root = args.run.resolve()
    protocol, state = read(root / 'protocol.json'), load_state(root)
    check_sources(protocol)
    report = dict(schema='ppo-start-curriculum-audit-v1', protocol_sha256=state['protocol_sha256'], jobs=[])
    for job in state['jobs']:
        records = [read(checked_path(root, r)) for r in job['records']]
        report['jobs'].append(dict(job=job['id'], **check_records(root, protocol, records)))
    args.output.parent.mkdir(parents=True, exist_ok=True)
    write(args.output, report)
    print(json.dumps(report))
