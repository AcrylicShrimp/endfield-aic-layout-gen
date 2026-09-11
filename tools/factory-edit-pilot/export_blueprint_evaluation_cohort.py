"""Copy the exact fixed evaluation cohort from a historical PPO protocol."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def export(protocol_path, output, expected):
    if output.exists():
        raise ValueError(f'fresh cohort output required: {output}')
    if sha(protocol_path) != expected:
        raise ValueError(f'source protocol hash differs: {protocol_path}')
    protocol = json.loads(protocol_path.read_text())
    frozen = protocol['spec']['frozen_starts']
    schedule = protocol['config']['evaluation']
    seeds = frozen['evaluation_seeds']
    if len(seeds) != schedule['attempts'] or len(set(seeds)) != len(seeds):
        raise ValueError(f'evaluation cohort cardinality differs: {protocol_path}')
    entries = []
    for index, seed in enumerate(seeds):
        entry = frozen['entries'][str(seed)]
        if entry['status'] != 'accepted' or entry['generation_seed'] != seed:
            raise ValueError(f'expected an admitted historical factory: {seed}')
        files = {}
        for name, ref in entry['files'].items():
            path = Path(ref['path'])
            if sha(path) != ref['sha256']:
                raise ValueError(f'historical factory input changed: {path}')
            files[name] = dict(path=f'{seed}/{name}.json', sha256=ref['sha256'], source_path=str(path))
        entries.append(dict(generation_seed=seed, action_seed=schedule['action_seed_base'] +
                            protocol['config']['seeds'][0] * 100000 + index,
                            initial_metrics=entry['generation']['initial_metrics'], files=files))
    output.mkdir(parents=True)
    shutil.copyfile(protocol_path, output / 'source-protocol.json')
    for entry in entries:
        for ref in entry['files'].values():
            target = output / ref['path']
            target.parent.mkdir(exist_ok=True)
            shutil.copyfile(ref['source_path'], target)
            if sha(target) != ref['sha256']:
                raise ValueError(f'copied cohort input changed: {target}')
    catalogs = {}
    for name, source in protocol['spec']['catalogs'].items():
        source = Path(source)
        catalogs[name] = dict(source_path=str(source), sha256=sha(source))
    value = dict(schema='blueprint-evaluation-cohort-v1', source_protocol=dict(path='source-protocol.json',
        sha256=expected, source_path=str(protocol_path)), source_horizon=protocol['config']['training']['horizon'],
        catalogs=catalogs, cases=entries)
    (output / 'cohort.json').write_text(json.dumps(value, indent=2) + '\n')
    print(json.dumps(dict(cases=len(entries), output=str(output), source_protocol_sha256=expected)))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--protocol', type=Path, required=True)
    parser.add_argument('--protocol-sha256', required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    export(args.protocol.resolve(), args.output.resolve(), args.protocol_sha256)


if __name__ == '__main__':
    main()
