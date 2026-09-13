"""Bounded normal-input and old-constructor comparison; never imported by training."""
import argparse
from copy import deepcopy
import json
from pathlib import Path
import subprocess
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from blueprint_ppo_store import read, sha, write
from blueprint_ppo_update import require
from declarative_learning_environment import Runtime, Stream, semantic
from declarative_learning_launch import sources
from declarative_learning_curriculum import first_start


def verify(launch_path, old_cli, output):
    output = Path(output).resolve()
    require(not output.exists(), f'construction.verify: fresh output required: {output}')
    output.mkdir(parents=True)
    began = time.monotonic()
    launch = read(launch_path)
    config = deepcopy(launch['config'])
    config['training_input'].update(directory=str(output / 'inputs'), seed_base=615700000)
    runtime = Runtime(config, launch['source']['native_library'])
    products = set().union(*[set(v) for v in config['strata'].values()])
    selected, rows = {}, []
    # At most one wave of fresh inputs, with no policy fitting or replay loading.
    for index in range(config['training']['streams']):
        require(time.monotonic() - began < 180, 'construction.verify: bounded input check exhausted')
        stream = Stream(runtime, index)
        stream.next_start = first_start(index)
        observation = stream.observe()
        saved = stream.snapshot()
        restored = Stream.restore(runtime, saved)
        require(semantic(restored.observe()) == semantic(observation), 'construction.verify: restored input differs')
        metadata = stream.metadata['training_input']
        root = runtime.inputs.root
        marker = root / metadata['reference']['path']
        record = read(marker)
        generation = read(root / record['attempts'][-1]['generation']['path'])
        admission = marker.parent / f'attempt-{len(record["attempts"])-1}' / 'admission'
        require(all(not (admission / name).exists() for name in
                    ('construction.json','construction.html','construction-process.stdout')),
                'construction.verify: normal generation wrote a report or stdout dump')
        rows.append(dict(product=metadata['draw']['product'], generation_seed=metadata['generation_seed'],
                         input=metadata['reference'], files=len(list(marker.parent.rglob('*'))),
                         input_bytes=sum(p.stat().st_size for p in marker.parent.rglob('*') if p.is_file())))
        selected.setdefault(metadata['draw']['product'], (admission, generation))
        if set(selected) == products:
            break
    require(set(selected) == products, 'construction.verify: product coverage exhausted')
    comparisons = []
    for product, (admission, generation) in selected.items():
        process = read(admission / 'construction-process.json')
        arguments = process['argv'][:]
        require(arguments.pop() == '--final-json', 'construction.verify: expected final-only caller')
        folder = output / 'comparison' / product
        folder.mkdir(parents=True)
        report_path, html_path = folder / 'report.json', folder / 'history.html'
        arguments.extend(['--report-output', str(report_path), '--visualization-output', str(html_path)])
        start = time.monotonic()
        result = subprocess.run([str(old_cli), *arguments], capture_output=True, text=True, timeout=30)
        old_seconds = time.monotonic() - start
        require(result.returncode == 0, f'construction.verify: previous constructor failed: {product}: {result.stderr}')
        old = read(report_path)
        initial = read(admission / 'input/initial.json')
        require(old['success'] and old['complete'] and all(old['final_node'][key] == initial[key]
                    for key in ('placements','transport_networks','logistics_components')),
                f'construction.verify: actual final geometry changed: {product}')
        comparisons.append(dict(product=product, geometry_identical=True,
            previous_output_bytes=report_path.stat().st_size + html_path.stat().st_size,
            previous_constructor_seconds=old_seconds, final_only_constructor_seconds=process['elapsed_ms']/1000))
        report_path.unlink()
        html_path.unlink()
    receipt = dict(schema='construction-output-verification-v1', status='passed',
        normal_input_roundtrip=True, final_output_comparison=True, inputs=rows, comparisons=comparisons,
        source=sources(launch['config'],launch['source']['native_library']),
        previous_cli=dict(path=str(Path(old_cli).resolve()),sha256=sha(old_cli)),
        seconds=time.monotonic()-began, scope='Only generated input and CLI output; no learner or historical replay.')
    write(output / 'verification.json', receipt)
    return dict(status='passed', products=sorted(products), inputs=len(rows), seconds=receipt['seconds'])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--launch', type=Path, required=True)
    parser.add_argument('--old-cli', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    print(verify(args.launch,args.old_cli,args.output))


if __name__ == '__main__':
    main()
