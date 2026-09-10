"""Compare retained PPO experience encodings without changing the running learner."""
import argparse
import gc
import gzip
import hashlib
import io
import json
from pathlib import Path
import pickle
import statistics
import time

import numpy as np
import torch


def sha(data):
    return hashlib.sha256(data).hexdigest()


def fingerprint(value):
    """Compare complete numeric content, independently of storage encoding."""
    digest = hashlib.sha256()

    def visit(item):
        if isinstance(item, torch.Tensor):
            digest.update(b'tensor')
            visit(item.detach().cpu().numpy())
        elif isinstance(item, np.ndarray):
            digest.update(b'array')
            visit((str(item.dtype), item.shape))
            digest.update(item.tobytes(order='C'))
        elif isinstance(item, dict):
            digest.update(b'dict')
            for key in sorted(item):
                visit(key)
                visit(item[key])
        elif isinstance(item, (list, tuple)):
            digest.update(type(item).__name__.encode())
            visit(len(item))
            for child in item:
                visit(child)
        else:
            digest.update(repr((type(item).__name__, item)).encode())
        digest.update(b'\0')

    visit(value)
    return digest.hexdigest()


class ObservedGzip(gzip.GzipFile):
    def __init__(self, packed):
        super().__init__(fileobj=io.BytesIO(packed), mode='rb')
        self.seeks = self.backward_seeks = 0

    def seek(self, offset, whence=io.SEEK_SET):
        before = self.tell() if whence != io.SEEK_CUR or offset else 0
        result = super().seek(offset, whence)
        if whence != io.SEEK_CUR or offset:
            self.seeks += 1
            self.backward_seeks += result < before
        return result


def stream_decode(packed):
    with gzip.GzipFile(fileobj=io.BytesIO(packed), mode='rb') as stream:
        return torch.load(stream, map_location='cpu', weights_only=False)


def main(args):
    if args.output.exists():
        raise ValueError(f'fresh output directory required: {args.output}')
    args.output.mkdir(parents=True)
    torch.set_num_threads(1)
    root = args.run.resolve()
    protocol_bytes = (root / 'protocol.json').read_bytes()
    state = json.loads((root / 'state.json').read_text())
    if len(state['jobs']) != 1:
        raise ValueError('this bounded probe requires exactly one retained job')
    reference = next(r for r in state['jobs'][0]['records']
                     if r['phase'] == dict(kind='training', batch=args.batch))
    record_bytes = (root / reference['path']).read_bytes()
    if sha(record_bytes) != reference['sha256']:
        raise ValueError(f'record hash mismatch: {root / reference["path"]}')
    record = json.loads(record_bytes)
    accepted = [r for r in record['results'] if r['status'] == 'complete']
    began = time.monotonic()

    def guard():
        if time.monotonic() - began > args.seconds:
            raise TimeoutError(f'data supply probe exceeded {args.seconds} seconds: {args.output}')

    report = dict(schema='ppo-data-supply-profile-v1', status='running', run=str(root), batch=args.batch,
                  source_sha256=sha(Path(__file__).read_bytes()), protocol_sha256=sha(protocol_bytes),
                  record=reference, behavior_checkpoint=record['behavior_checkpoint'],
                  repetitions=args.repeats, torch_version=torch.__version__, samples=[], batch_cells={})

    def publish():
        (args.output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')

    packed_inputs, pickle_inputs, expected = [], [], []
    for result in accepted:
        guard()
        path = Path(result['experience'])
        packed = path.read_bytes()
        if sha(packed) != result['experience_sha256']:
            raise ValueError(f'experience hash mismatch: {path}')
        payload = stream_decode(packed)
        packed_inputs.append(packed)
        pickle_inputs.append(pickle.dumps(payload, protocol=5))
        expected.append(fingerprint(payload))
    report['inputs'] = [dict(path=r['experience'], sha256=r['experience_sha256'],
                            semantic_sha256=identity) for r, identity in zip(accepted, expected)]
    report['compressed_bytes'] = sum(map(len, packed_inputs))
    report['pickle_bytes'] = sum(map(len, pickle_inputs))

    cells = {
        'gzip-torch-stream': (packed_inputs, stream_decode),
        'inflate-once-torch': (packed_inputs, lambda b: torch.load(io.BytesIO(gzip.decompress(b)),
                                                                 map_location='cpu', weights_only=False)),
        'memory-pickle5': (pickle_inputs, pickle.loads),
    }
    # All cells preserve input order. Alternate cell order across repetitions to
    # reduce timing bias from the concurrently running training experiment.
    timings = {name: [] for name in cells}
    for repetition in range(args.repeats + 1):
        order = list(cells)
        if repetition % 2:
            order.reverse()
        for name in order:
            guard()
            inputs, decode = cells[name]
            gc.collect()
            start = time.perf_counter()
            payloads = [decode(data) for data in inputs]
            seconds = time.perf_counter() - start
            if [fingerprint(p) for p in payloads] != expected:
                raise AssertionError(f'decoded content changed: {name}')
            del payloads
            if repetition:
                timings[name].append(seconds)
                report['batch_cells'][name] = dict(seconds=timings[name], median_seconds=statistics.median(timings[name]),
                                                   all_content_exact=True)
            publish()
            print(json.dumps(dict(cell=name, repetition=repetition, seconds=seconds)), flush=True)

    indices = sorted(range(len(accepted)), key=lambda i: len(packed_inputs[i]))
    for index in (indices[len(indices) // 2], indices[-1]):
        guard()
        packed = packed_inputs[index]
        with ObservedGzip(packed) as stream:
            payload = torch.load(stream, map_location='cpu', weights_only=False)
            counts = dict(seeks=stream.seeks, backward_seeks=stream.backward_seeks)
        assert fingerprint(payload) == expected[index]
        sample = dict(path=accepted[index]['experience'], compressed_bytes=len(packed),
                      expanded_torch_bytes=len(gzip.decompress(packed)), pickle_bytes=len(pickle_inputs[index]),
                      torch_gzip_access=counts, stages={})
        for _ in range(args.repeats):
            start = time.perf_counter()
            raw = gzip.decompress(packed)
            inflate = time.perf_counter() - start
            start = time.perf_counter()
            decoded = torch.load(io.BytesIO(raw), map_location='cpu', weights_only=False)
            deserialize = time.perf_counter() - start
            start = time.perf_counter()
            serialized = pickle.dumps(decoded, protocol=5)
            serialize_pickle = time.perf_counter() - start
            start = time.perf_counter()
            roundtrip = pickle.loads(serialized)
            deserialize_pickle = time.perf_counter() - start
            assert fingerprint(roundtrip) == expected[index]
            for key, value in dict(inflate=inflate, deserialize_torch=deserialize,
                                   serialize_pickle5=serialize_pickle, deserialize_pickle5=deserialize_pickle).items():
                sample['stages'].setdefault(key, []).append(value)
        report['samples'].append(sample)
        publish()
    report.update(status='complete', seconds=time.monotonic() - began)
    publish()


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run', type=Path, required=True)
    parser.add_argument('--batch', type=int, default=3)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--repeats', type=int, choices=range(1, 6), default=3)
    parser.add_argument('--seconds', type=int, choices=range(30, 1801), default=600)
    main(parser.parse_args())
