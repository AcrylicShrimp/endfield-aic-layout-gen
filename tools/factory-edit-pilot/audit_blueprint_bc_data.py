"""Independently restore and replay every retained BC demonstration branch."""
import argparse
import json
from pathlib import Path
import tempfile

from audit_blueprint_ppo import WitnessAudit
from blueprint_edit_input import prepare
from blueprint_ppo import check_sources
from blueprint_ppo_environment import Runtime, semantic
from blueprint_ppo_store import ROOT, checked, load, read, sha
from blueprint_ppo_update import digest, require
from build_blueprint_bc_data import before_checkpoint, replay
from post_pose_recovery import draft


def audit_dataset(path, protocol=None):
    dataset = load(path)
    require(dataset['schema'] == 'blueprint-bc-dataset-v1', f'blueprint.bc-audit: dataset schema: {path}')
    require(dataset['build_source_sha256'] == sha(Path(__file__).with_name('build_blueprint_bc_data.py')), 'blueprint.bc-audit: builder source')
    source, recovery = [Path(dataset[k]) for k in ('source_run', 'recovery_run')]
    source_protocol = read(checked(dataset['source_protocol'], source))
    source_audit = read(checked(dataset['source_audit'], source))
    require(source_audit['status'] == 'passed', 'blueprint.bc-audit: source audit')
    check_sources(source_protocol)
    census = read(checked(dataset['recovery_report'], recovery))
    require(set(dataset['candidates']) == {r['name'] for r in census['cases'] if r['improves_area']}, 'blueprint.bc-audit: complete candidate census')
    if protocol is not None:
        require(protocol['dataset_sha256'] == sha(path) and protocol['native_sha256'] == dataset['native_sha256'], 'blueprint.bc-audit: dataset/native run binding')
    runtime = Runtime(source_protocol['config'], source_protocol['native_library'])
    groups = {'train': set(), 'holdout': set()}
    physical_inputs = {'train': set(), 'holdout': set()}
    ids = set()
    with tempfile.TemporaryDirectory(prefix='blueprint-bc-data-audit-') as temporary:
        checker = WitnessAudit(runtime, temporary)
        for demo in dataset['demonstrations']:
            require(demo['id'] not in ids, 'blueprint.bc-audit: unique demonstrations')
            ids.add(demo['id'])
            groups[demo['split']].add(demo['group'])
            first_source = demo['sources'][0]
            case = load(checked(first_source['source_case'], recovery))
            saved = before_checkpoint(source, case)
            require(digest(saved) == digest(demo['checkpoint']), 'blueprint.bc-audit: authenticated pre-pose checkpoint')
            for reference in demo['sources']:
                duplicate = load(checked(reference['source_case'], recovery))
                cp = before_checkpoint(source, duplicate)
                group = digest(dict(draft=draft(duplicate['before']), best=cp['best_draft']))
                require(group == demo['group'] and digest(dict(group=group, action=duplicate['action'])) == demo['id'], 'blueprint.bc-audit: physical deduplication identity')
            require(demo['after']['score'][0] < min(demo['before']['score'][0], demo['best_score'][0]), 'blueprint.bc-audit: incremental area contribution')
            require([r['action']['kind'] for r in demo['after']['rows']] == ['pose', 'repair_all', 'commit'], 'blueprint.bc-audit: complete labels')
            for row in demo['after']['rows']:
                observation = row['observation']
                physical_inputs[demo['split']].add(digest(dict(draft=draft(observation), best=observation['best_metrics'])))
            for branch in ('before', 'after'):
                original = demo[branch]
                actual = replay(runtime, saved, [r['action'] for r in original['rows']], checker)
                require(actual['status'] == 'validated' and actual['score'] == original['score'], 'blueprint.bc-audit: repeated physical recovery score')
                require(digest(actual['witness']) == digest(original['witness']), 'blueprint.bc-audit: repeated witness identity')
                for a, b in zip(actual['rows'], original['rows'], strict=True):
                    require(semantic(a['observation']) == semantic(b['observation']), 'blueprint.bc-audit: restored sequential observations')
                    require(digest(prepare(b['observation'])) == digest(b['input']), 'blueprint.bc-audit: recorded tensor input')
                    require(b['input']['actions'][b['index']] == b['action'], 'blueprint.bc-audit: candidate label identity')
                    for key in ('status', 'reward', 'charged', 'best_metrics', 'revision'):
                        require(a['outcome'][key] == b['outcome'][key], f'blueprint.bc-audit: branch outcome {key}')
        require(groups['train'] and groups['holdout'] and groups['train'].isdisjoint(groups['holdout']), 'blueprint.bc-audit: nonempty disjoint physical splits')
        require(physical_inputs['train'].isdisjoint(physical_inputs['holdout']), 'blueprint.bc-audit: no cross-split physical label inputs')
    return dataset


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--dataset', type=Path, required=True)
    args = parser.parse_args()
    dataset = audit_dataset(args.dataset.resolve())
    print(json.dumps(dict(status='passed', demonstrations=len(dataset['demonstrations']), dataset_sha256=sha(args.dataset))))
