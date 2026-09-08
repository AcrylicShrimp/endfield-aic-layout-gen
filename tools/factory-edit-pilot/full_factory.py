"""Check or replay complete production-witness proposals through Rust acceptance."""
import argparse
import hashlib
import json
from pathlib import Path

from native import load_native


def write(path, value):
    with path.open('x') as stream:
        json.dump(value, stream)
        stream.write('\n')


def run(args):
    root = args.root.resolve()
    module = load_native(args.native_library)
    if not hasattr(module, 'FactoryEditSession'):
        raise ValueError(f'factory.native-schema: {args.native_library}: rebuild with full-factory support')
    build = json.loads(module.Engine.build_info_json())
    for relative, expected in build['files_sha256'].items():
        path = root / relative
        if not path.is_file() or hashlib.sha256(path.read_bytes()).hexdigest() != expected:
            raise ValueError(f'factory.stale-build: {path}: rebuild the native library')
    budget = {'proposals': args.max_proposals, 'elapsed_ms': args.elapsed_ms}
    session = module.FactoryEditSession.open(str(root), str(args.request.resolve()), str(args.initial.resolve()),
                                            args.catalogs.read_text(), json.dumps(budget))
    args.output.mkdir()
    initial = json.loads(session.snapshot_json())
    rows = []
    input_guard_exhausted = False
    if args.command == 'replay':
        with args.proposals.open() as stream, (args.output / 'trace.jsonl').open('x') as trace:
            while line := stream.readline(16 * 1024 * 1024 + 1):
                # The Rust parser charges and validates every selected line, including bad JSON.
                row = json.loads(session.propose_json(line))
                trace.write(json.dumps(row) + '\n')
                rows.append(row)
                if len(line) > 16 * 1024 * 1024:
                    input_guard_exhausted = True
                    break
                if row['status'] == 'budget_exhausted':
                    break
    final = json.loads(session.snapshot_json())
    write(args.output / 'initial.json', final.pop('initial'))
    write(args.output / 'best.json', final.pop('best'))
    write(args.output / 'current.json', final.pop('current'))
    proposal_source = None
    if args.command == 'replay':
        with args.proposals.open('rb') as stream:
            proposal_source = {'path': str(args.proposals.resolve()), 'sha256': hashlib.file_digest(stream, 'sha256').hexdigest()}
    report = {'schema_version': 1, 'command': args.command,
              'initial_metrics': initial['initial_metrics'], 'final': final,
              'input_guard_exhausted': input_guard_exhausted,
              'driver_sha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              'proposal_source': proposal_source,
              'statuses': {status: sum(r['status'] == status for r in rows) for status in sorted({r['status'] for r in rows})},
              'metadata': json.loads(session.metadata_json()),
              'library': {'path': str(args.native_library.resolve()),
                          'sha256': hashlib.sha256(args.native_library.read_bytes()).hexdigest()}}
    write(args.output / 'report.json', report)
    print(json.dumps({'status': 'complete', 'factory_validated': True,
                      'initial': initial['initial_metrics']['score'], 'best': final['best_metrics']['score'],
                      'proposals': final['proposals'], 'output': str(args.output)}))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('command', choices=('check', 'replay'))
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument('--request', type=Path, required=True)
    parser.add_argument('--initial', type=Path, required=True)
    parser.add_argument('--catalogs', type=Path, required=True)
    parser.add_argument('--native-library', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--proposals', type=Path)
    parser.add_argument('--max-proposals', type=int, default=128)
    parser.add_argument('--elapsed-ms', type=int, default=30000)
    args = parser.parse_args()
    if args.command == 'replay' and args.proposals is None:
        parser.error('replay requires --proposals JSONL')
    try:
        run(args)
    except (OSError, ValueError) as error:
        print(json.dumps({'code':'factory.cli-failed', 'severity':'error', 'stage':'factory-cli', 'message':str(error)}))
        raise SystemExit(1)
