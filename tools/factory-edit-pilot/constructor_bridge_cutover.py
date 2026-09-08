"""Replay an original factory request with the production channel-aware constructor."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import time
from constructor_attachment_trace import read, sha


def run(args):
    attempt = args.attempt.resolve()
    original = read(attempt / 'construction-process.json')
    baseline = read(attempt / 'construction.json')
    args.output.mkdir()
    output = args.output.resolve()
    argv = list(original['argv'])
    for flag, name in [('--report-output', 'construction.json'), ('--visualization-output', 'construction.html')]:
        argv[argv.index(flag)+1] = str(output / name)
    inputs = [Path(argv[argv.index(flag)+1]) for flag in ('--recipes', '--source-plan', '--facility-catalog',
        '--item-catalog', '--transport-catalog', '--logistics-component-catalog')]
    # Runtime catalogs must still equal the earlier admission's frozen inputs.
    previous = {p['path']: p['sha256'] for p in read(attempt.parent / 'report.json')['provenance']}
    for path in inputs:
        if str(path) in previous and sha(path) != previous[str(path)]:
            raise ValueError(f'bridge.changed-original-input: {path}')
    root = Path(__file__).resolve().parents[2]
    sources = [*inputs, attempt / 'construction.json', attempt / 'construction-process.json',
        args.binary, Path(__file__), root / 'Cargo.toml', root / 'Cargo.lock']
    sources.extend((root / 'crates/aic-data/src').rglob('*.rs'))
    sources.extend((root / 'crates/aic-cli/src').rglob('*.rs'))
    hashes = {str(path.resolve()): sha(path) for path in sources}
    (output / 'protocol.json').write_text(json.dumps(dict(schema_version=1, baseline=str(attempt),
        binary=str(args.binary.resolve()), argv=argv, seconds=60, source_sha256=hashes,
        scope='Complete original request, unchanged rates and operational budgets; new production bridge-channel semantics'), indent=2)+'\n')
    began = time.perf_counter(); timed_out = False
    env = dict(os.environ)
    env.pop('AIC_ATTACHMENT_DIAGNOSTIC', None)
    with (output / 'process.stdout').open('wb') as stdout, (output / 'process.stderr').open('wb') as stderr:
        try:
            result = subprocess.run([str(args.binary.resolve()), *argv], env=env,
                timeout=60, stdout=stdout, stderr=stderr, check=False)
            code = result.returncode
        except subprocess.TimeoutExpired:
            timed_out = True; code = None
    for path, digest in hashes.items():
        if sha(path) != digest:
            raise ValueError(f'bridge.changed-source-during-run: {path}')
    report = dict(schema_version=1, code=code, timed_out=timed_out,
        seconds=time.perf_counter()-began, baseline_covered=len(baseline['final_node']['member_instances']))
    if not timed_out:
        if code not in (0, 1):
            raise ValueError(f'bridge.unexpected-exit: {code}; see {output}')
        actual = read(output / 'construction.json')
        report.update(complete=actual['complete'], status=actual['status'], required=actual['required_facilities'],
            covered=len(actual['final_node']['member_instances']), failed_statistics=actual['failed_statistics'],
            rejected_reasons=actual['rejected_reasons'], construction_sha256=sha(output / 'construction.json'))
    (output / 'report.json').write_text(json.dumps(report, indent=2)+'\n')
    print(json.dumps(report))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('attempt', 'binary', 'output'):
        parser.add_argument('--'+name, type=Path, required=True)
    run(parser.parse_args())
