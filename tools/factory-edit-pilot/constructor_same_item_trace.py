"""Compare the isolated crossing-candidate intervention with a frozen trace replay."""
import argparse
from collections import Counter
import json
import os
from pathlib import Path
import subprocess
import time
from constructor_attachment_trace import read, sha, stable


def run(args):
    baseline = args.baseline.resolve()
    protocol = read(baseline / 'protocol.json')
    for path, digest in protocol['source_sha256'].items():
        if sha(path) != digest:
            raise ValueError(f'attachment.changed-baseline-source: {path}')
    args.output.mkdir()
    output = args.output.resolve()
    config = read(baseline / 'trace-config.json')
    config.update(output=str(output / 'trials.jsonl'), snapshot=str(output / 'snapshot.json'))
    (output / 'trace-config.json').write_text(json.dumps(config, indent=2) + '\n')
    argv = list(protocol['argv'])
    for flag, name in [('--report-output', 'construction.json'), ('--visualization-output', 'construction.html')]:
        argv[argv.index(flag) + 1] = str(output / name)
    sources = {str(p.resolve()): sha(p) for p in [args.binary, args.patch, Path(__file__),
        Path(__file__).with_name('constructor_attachment_trace.py'), baseline / 'protocol.json',
        baseline / 'construction.json', output / 'trace-config.json']}
    (output / 'protocol.json').write_text(json.dumps(dict(schema_version=1,
        intervention='same-item-crossing-candidates-only', baseline=str(baseline),
        source_sha256=sources, argv=argv, seconds=60), indent=2) + '\n')
    began = time.perf_counter()
    with (output / 'process.stdout').open('wb') as stdout, (output / 'process.stderr').open('wb') as stderr:
        result = subprocess.run([str(args.binary.resolve()), *argv], timeout=60, check=False,
            env=dict(os.environ, AIC_ATTACHMENT_DIAGNOSTIC=str(output / 'trace-config.json')),
            stdout=stdout, stderr=stderr)
    if result.returncode not in (0, 1):
        raise ValueError(f'attachment.unexpected-exit: {result.returncode}; see {output}')
    actual = read(output / 'construction.json')
    trials = [json.loads(line) for line in (output / 'trials.jsonl').read_text().splitlines()]
    if not trials or len(trials) > config['max_rows']:
        raise ValueError('attachment.trace-membership')
    for i, row in enumerate(trials):
        if row['ordinal'] != i or row['edge'] != config['edge']:
            raise ValueError('attachment.trace-order')
    for path, digest in sources.items():
        if sha(path) != digest:
            raise ValueError(f'attachment.changed-input: {path}')
    report = dict(schema_version=1, status='measured', code=result.returncode,
        seconds=time.perf_counter()-began, trials=len(trials),
        paths=dict(Counter(str(row['path_found']) for row in trials)),
        baseline_reproduced=stable(actual)==stable(read(baseline / 'construction.json')),
        protocol_sha256=sha(output / 'protocol.json'), snapshot_sha256=sha(output / 'snapshot.json'),
        trial_sha256=sha(output / 'trials.jsonl'),
        scope='Changed crossing candidate generation; unchanged factory validators; partial states are diagnostic only')
    (output / 'report.json').write_text(json.dumps(report, indent=2)+'\n')
    print(json.dumps(report))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('baseline', 'binary', 'patch', 'output'):
        parser.add_argument('--'+name, required=True, type=Path)
    run(parser.parse_args())
