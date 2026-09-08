"""Bounded growth-prefix diagnostic; never an admitted complete factory."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import time
from constructor_attachment_trace import read, sha


def run(args):
    if not 1 <= args.steps <= 256:
        raise ValueError('bridge.prefix-step-bound: expected 1..256')
    args.output.mkdir()
    output = args.output.resolve()
    process = read(args.attempt / 'construction-process.json')
    argv = list(process['argv'])
    original_steps = argv[argv.index('--max-steps')+1]
    argv[argv.index('--max-steps')+1] = str(args.steps)
    for flag, filename in [('--report-output', 'construction.json'), ('--visualization-output', 'construction.html')]:
        argv[argv.index(flag)+1] = str(output / filename)
    sources = [args.binary, Path(__file__), args.attempt / 'construction-process.json']
    sources.extend(Path(argv[argv.index(flag)+1]) for flag in ('--recipes', '--source-plan', '--facility-catalog',
        '--item-catalog', '--transport-catalog', '--logistics-component-catalog'))
    hashes = {str(p.resolve()): sha(p) for p in sources}
    (output / 'protocol.json').write_text(json.dumps(dict(schema_version=1,
        scope='Diagnostic prefix of the full original wiring; never a full-factory admission',
        original_steps=original_steps, diagnostic_steps=args.steps, seconds=60,
        argv=argv, source_sha256=hashes), indent=2)+'\n')
    began = time.perf_counter(); code = None; timed_out = False
    env = dict(os.environ); env.pop('AIC_ATTACHMENT_DIAGNOSTIC', None)
    with (output / 'process.stdout').open('wb') as stdout, (output / 'process.stderr').open('wb') as stderr:
        try:
            code = subprocess.run([str(args.binary.resolve()), *argv], timeout=60, check=False,
                stdout=stdout, stderr=stderr, env=env).returncode
        except subprocess.TimeoutExpired:
            timed_out = True
    for p, digest in hashes.items():
        if sha(p) != digest: raise ValueError(f'bridge.prefix-source-changed: {p}')
    result = dict(schema_version=1, diagnostic_only=True, factory_admitted=False,
        code=code, timed_out=timed_out, seconds=time.perf_counter()-began)
    if not timed_out:
        report = read(output / 'construction.json')
        result.update(status=report['status'], required=report['required_facilities'],
            covered=len(report['final_node']['member_instances']), steps=len(report['steps']),
            rejected_reasons=report['rejected_reasons'], construction_sha256=sha(output / 'construction.json'))
    (output / 'report.json').write_text(json.dumps(result, indent=2)+'\n'); print(json.dumps(result))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('attempt', 'binary', 'output'):
        parser.add_argument('--'+name, required=True, type=Path)
    parser.add_argument('--steps', required=True, type=int)
    run(parser.parse_args())
