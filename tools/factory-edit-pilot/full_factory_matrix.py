"""Exercise complete-factory acceptance over every explicitly supplied original request."""
import argparse
import hashlib
import json
from pathlib import Path
import time

from native import load_native
from full_factory import write


def run(args):
    spec = json.loads(args.inputs.read_text())
    root = Path(__file__).resolve().parents[2]
    module = load_native(args.native_library)
    build = json.loads(module.Engine.build_info_json())
    for relative, expected in build['files_sha256'].items():
        if hashlib.sha256((root / relative).read_bytes()).hexdigest() != expected:
            raise ValueError(f'factory.stale-build: {relative}')
    catalogs = Path(spec['catalogs']).read_text()
    args.output.mkdir()
    rows = []
    started = time.perf_counter()
    for case in spec['rows']:
        directory = args.output / case['id']
        directory.mkdir()
        session = module.FactoryEditSession.open(str(root), case['request'], case['initial'], catalogs,
                                                json.dumps({'proposals': 3, 'elapsed_ms': 30000}))
        initial = json.loads(session.snapshot_json())
        accepted = json.loads(session.propose_json(Path(case['initial']).read_text()))
        malformed = json.loads(session.propose_json('not-json'))
        removed = json.loads(Path(case['initial']).read_text())
        removed['transport_networks'] = []
        removed['logistics_components'] = []
        incomplete = json.loads(session.propose_json(json.dumps(removed)))
        final = json.loads(session.snapshot_json())
        if (accepted['status'] != 'accepted_equal' or malformed['status'] != 'rejected'
                or incomplete['status'] != 'rejected' or final['best'] != initial['best']
                or final['current'] != initial['current'] or final['proposals'] != 3):
            raise ValueError(f'factory.transaction-invariant: {case["id"]}')
        write(directory / 'best.json', final['best'])
        record = {'id': case['id'], 'instances': case['instances'],
                  'metrics': final['best_metrics'], 'initialization_ms': initial['initialization_ms'],
                  'outcomes': [accepted, malformed, incomplete], 'best': str(directory / 'best.json'),
                  'metadata': json.loads(session.metadata_json())}
        write(directory / 'report.json', record)
        rows.append({key: record[key] for key in ('id','instances','metrics','initialization_ms','best')})
        print(json.dumps({'case': case['id'], 'score': record['metrics']['score'], 'status': 'checked'}), flush=True)
    report = {'schema_version': 1, 'status': 'complete', 'rows': rows, 'seconds': time.perf_counter()-started,
              'inputs': {'path': str(args.inputs.resolve()), 'sha256': hashlib.sha256(args.inputs.read_bytes()).hexdigest()},
              'driver_sha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              'scope': 'complete-factory validation and transaction checks; no learning or policy evaluation'}
    write(args.output / 'matrix.json', report)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--inputs', type=Path, required=True)
    parser.add_argument('--native-library', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    try:
        run(args)
    except (ValueError, OSError) as error:
        print(json.dumps({'code':'factory.matrix-failed','severity':'error','stage':'factory-matrix','message':str(error)}))
        raise SystemExit(1)
