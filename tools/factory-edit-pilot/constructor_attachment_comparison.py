"""Audit source identity and isolate the selected-edge routing intervention."""
import argparse
from collections import Counter
import json
from pathlib import Path
from constructor_attachment_trace import read, sha, stable


def run(args):
    rows = []; snapshots = []; constructions = []
    for directory in (args.baseline, args.global_intervention, args.selected_intervention):
        protocol = read(directory / 'protocol.json')
        for path, digest in protocol['source_sha256'].items():
            if sha(path) != digest:
                raise ValueError(f'attachment.changed-source: {path}')
        for source in protocol.get('baseline_provenance', []):
            if sha(source['path']) != source['sha256']:
                raise ValueError(f'attachment.changed-catalog: {source["path"]}')
        report = read(directory / 'report.json')
        for name, key in [('protocol.json', 'protocol_sha256'), ('snapshot.json', 'snapshot_sha256'),
                ('trials.jsonl', 'trial_sha256')]:
            if sha(directory / name) != report[key]:
                raise ValueError(f'attachment.changed-output: {directory / name}')
        construction = read(directory / 'construction.json')
        snapshot = read(directory / 'snapshot.json')
        trials = [json.loads(line) for line in (directory / 'trials.jsonl').read_text().splitlines()]
        if len(trials) != report['trials'] or any(r['ordinal'] != i or r['edge'] != snapshot['edge']['id'] for i, r in enumerate(trials)):
            raise ValueError('attachment.trial-membership')
        constructions.append(construction); snapshots.append(snapshot)
        rows.append(dict(directory=str(directory.resolve()), seconds=report['seconds'],
            paths=dict(Counter(str(r['path_found']) for r in trials)),
            status=construction['status'], complete=construction['complete'],
            required=construction['required_facilities'], covered=len(construction['final_node']['member_instances']),
            failed_statistics=construction['failed_statistics'], rejected_reasons=construction['rejected_reasons']))
    original = read(Path(read(args.baseline / 'protocol.json')['attempt']) / 'construction.json')
    if stable(original) != stable(constructions[0]):
        raise ValueError('attachment.original-baseline-mismatch')
    for key in ('steps', 'final_node', 'request'):
        if stable(constructions[0][key]) != stable(constructions[2][key]):
            raise ValueError(f'attachment.selected-prior-state-mismatch: {key}')
    for key in ('node', 'edge', 'blocked', 'facility_cells', 'layer_cells'):
        if snapshots[0][key] != snapshots[2][key]:
            raise ValueError(f'attachment.selected-snapshot-mismatch: {key}')
    for key in ('source', 'target', 'width', 'height', 'lane'):
        if snapshots[0]['trial'][key] != snapshots[2]['trial'][key]:
            raise ValueError(f'attachment.selected-endpoint-mismatch: {key}')
    result = dict(schema_version=1, status='passed', baseline_exact_non_timing=True,
        selected_prior_states_and_first_trial_geometry_identical=True,
        global_first_trial_geometry_identical=snapshots[0]['node']==snapshots[1]['node'],
        selected_first_path=snapshots[2]['path'], rows=rows,
        scope='Source/hash and controlled-state comparison; constructor candidates retain unchanged native validators')
    with args.output.open('x') as output:
        output.write(json.dumps(result, indent=2)+'\n')
    print(json.dumps({k:v for k,v in result.items() if k!='rows'}))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('baseline', 'global-intervention', 'selected-intervention', 'output'):
        parser.add_argument('--'+name, required=True, type=Path)
    run(parser.parse_args())
