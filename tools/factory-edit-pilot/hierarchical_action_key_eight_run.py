"""Run the six frozen paired searches with per-process RSS measurement."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time

root = Path('artifacts/gumbel-action-key-eight')
protocol = json.loads((root / 'protocol.json').read_text())
expected = protocol['binary_sha256']
actual = hashlib.sha256(Path('target/release/aic-cli').read_bytes()).hexdigest()
if actual != expected:
    raise ValueError('release binary changed')
for name in protocol['runs']:
    arm, case_name = name.split('-', 1)
    if case_name == 'repeat':
        case_name = 'original'
    config = (Path('docs/benchmarks/hierarchical-target-causal/c16-n128-search.json')
              if arm == 'rank' else root / 'search-key.json')
    outer = (Path('artifacts/recovery-search-v2/inputs/models/outer-128')
             if arm == 'rank' else root / 'outer-key')
    output = root / f'report-{name}.json'
    metrics = root / f'metrics-{name}.json'
    if output.exists() or metrics.exists():
        raise ValueError(f'output already exists for {name}')
    command = [
        'target/release/aic-cli', 'learning', 'hierarchical-search-harness',
        '--config', str(config), '--panel', str(root / f'panel-{case_name}.json'),
        '--lower-training-output', 'artifacts/recovery-search-v2/inputs/models/lower',
        '--outer-training-output', str(outer), '--catalog-root', 'data/game/normalized',
        '--output', str(output), '--device', 'cuda:0',
    ]
    with (root / f'run-{name}.log').open('w') as log:
        start = time.perf_counter()
        child = subprocess.Popen(command, stdout=log, stderr=subprocess.STDOUT)
        _, wait_status, usage = os.wait4(child.pid, 0)
        elapsed = time.perf_counter() - start
        child.returncode = os.waitstatus_to_exitcode(wait_status)
    if child.returncode:
        raise RuntimeError(f'{name} exited {child.returncode}')
    report = json.loads(output.read_text())
    if report['status'] != 'complete' or len(report['cases']) != protocol['cases']:
        raise ValueError(f'{name} report incomplete')
    metrics.write_text(json.dumps({
        'name': name, 'wall_seconds': elapsed, 'maximum_rss_kib': usage.ru_maxrss,
        'report_sha256': hashlib.sha256(output.read_bytes()).hexdigest(),
    }, indent=2) + '\n')
    print(name, round(elapsed, 3), usage.ru_maxrss, flush=True)
