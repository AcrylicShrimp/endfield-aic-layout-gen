"""Attribute retained PPO array bytes and exact repeated content by field."""
import argparse
from collections import Counter
import gzip
import hashlib
import io
import json
from pathlib import Path
import pickle

import numpy as np
import torch


def main(args):
    if args.output.exists():
        raise ValueError(f'fresh report path required: {args.output}')
    torch.set_num_threads(1)
    state = json.loads((args.run / 'state.json').read_text())
    reference = next(r for r in state['jobs'][0]['records']
                     if r['phase'] == dict(kind='training', batch=args.batch))
    data = (args.run / reference['path']).read_bytes()
    assert hashlib.sha256(data).hexdigest() == reference['sha256']
    record = json.loads(data)
    fields, shapes, totals, episodes = Counter(), Counter(), Counter(), []
    for row in record['results']:
        if row['status'] != 'complete':
            continue
        packed = Path(row['experience']).read_bytes()
        assert hashlib.sha256(packed).hexdigest() == row['experience_sha256']
        payload = torch.load(io.BytesIO(gzip.decompress(packed)), weights_only=False, map_location='cpu')
        seen, content, counts = set(), set(), Counter()

        def visit(value, path):
            if isinstance(value, np.ndarray):
                counts['referenced_array_bytes'] += value.nbytes
                if id(value) in seen:
                    return
                seen.add(id(value))
                counts['unique_object_array_bytes'] += value.nbytes
                fields[path] += value.nbytes
                shapes[f'{path}:{value.dtype}:{value.shape}'] += 1
                raw = value.tobytes(order='C')
                key = (str(value.dtype), value.shape, hashlib.sha256(raw).digest())
                if key not in content:
                    content.add(key)
                    counts['unique_content_array_bytes'] += value.nbytes
                else:
                    counts['duplicate_content_array_bytes'] += value.nbytes
                if path.endswith('.raster'):
                    counts['raster_nonzero_elements'] += int(np.count_nonzero(value))
                    counts['raster_elements'] += value.size
            elif isinstance(value, dict):
                for key, child in value.items():
                    visit(child, f'{path}.{key}' if path else key)
            elif isinstance(value, (list, tuple)):
                for child in value:
                    visit(child, path + '[]')

        visit(payload, '')
        counts.update(steps=len(payload['steps']), compressed_bytes=len(packed),
                      pickle_bytes=len(pickle.dumps(payload, protocol=5)))
        totals.update(counts)
        episodes.append(dict(case=row['row']['case'], **counts))
    report = dict(schema='ppo-payload-profile-v1', run=str(args.run), batch=args.batch,
                  record=reference, source_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                  totals=dict(totals), fields=dict(fields.most_common()),
                  common_shapes=shapes.most_common(20), episodes=episodes)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(dict(totals=report['totals'], fields=report['fields'], shapes=report['common_shapes'])))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run', type=Path, required=True)
    parser.add_argument('--batch', type=int, default=3)
    parser.add_argument('--output', type=Path, required=True)
    main(parser.parse_args())
