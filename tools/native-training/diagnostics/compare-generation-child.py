"""Bounded same-input CLI launch comparison with and without learning linkage."""
import argparse
import hashlib
import json
from pathlib import Path
import statistics
import subprocess
import time


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('input', type=Path, help='Actual generated input.json')
    parser.add_argument('normal', type=Path)
    parser.add_argument('without_learning', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    args.output.mkdir(parents=True)
    actual = json.loads(args.input.read_text())
    draw = actual['metadata']['draw']
    source = Path(draw['source'])
    plan = json.loads(source.read_text())
    plan['target']['quantity'] = draw['rate']['quantity']
    plan['target']['duration_ms'] = draw['rate']['duration_ms']
    plan_path = args.output / 'source-plan.json'
    plan_path.write_text(json.dumps(plan)+'\n')
    command = ['recipes','contextual-instance-wiring','--file',
               'data/game/normalized/recipes.json','--request',str(plan_path.resolve())]
    rows = {name: [] for name in ('normal','without_learning')}
    reference = None
    # Alternate order to avoid assigning every earlier observation to one arm.
    for pair in range(6):
        order = ('normal','without_learning') if pair % 2 == 0 else ('without_learning','normal')
        for name in order:
            binary = getattr(args, name).resolve()
            began = time.perf_counter()
            result = subprocess.run([str(binary),*command],capture_output=True,check=True,timeout=30)
            elapsed = time.perf_counter()-began
            value = json.loads(result.stdout)
            if reference is None:
                reference = value
            if value != reference or not value.get('success'):
                raise ValueError('Same-input command results differ or failed')
            rows[name].append(elapsed)
    output = dict(schema='aic-generation-child-comparison-v1',
        input=dict(path=str(args.input),sha256=hashlib.sha256(args.input.read_bytes()).hexdigest()),
        source=dict(path=str(source),sha256=hashlib.sha256(source.read_bytes()).hexdigest()),
        command=command,outputs_equal=True,
        binaries={name:dict(path=str(getattr(args,name)),
            sha256=hashlib.sha256(getattr(args,name).read_bytes()).hexdigest(),
            seconds=values,median_seconds=statistics.median(values)) for name,values in rows.items()},
        scope='Isolated process launch plus real wiring command; no training or claimed whole-run speedup.')
    (args.output/'summary.json').write_text(json.dumps(output,indent=2)+'\n')
    print(json.dumps({name:statistics.median(values) for name,values in rows.items()}))
