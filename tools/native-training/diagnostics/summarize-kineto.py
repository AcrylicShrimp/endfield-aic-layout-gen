"""Offline summary of an explicit Kineto trace; never imported by training."""
import argparse
from collections import defaultdict
import json
from pathlib import Path


def union_us(intervals):
    total = 0.0
    end = float('-inf')
    for start, stop in sorted(intervals):
        total += max(0.0, stop - max(start, end))
        end = max(end, stop)
    return total


def grouped(events, limit=25):
    rows = defaultdict(lambda: [0, 0.0])
    for event in events:
        row = rows[event['name']]
        row[0] += 1
        row[1] += event['dur']
    return [dict(name=name, calls=n, seconds=us / 1e6, mean_us=us / n)
            for name, (n, us) in sorted(rows.items(), key=lambda x: -x[1][1])[:limit]]


def summarize(trace):
    events = [e for e in trace['traceEvents'] if e.get('ph') == 'X' and e.get('dur', 0) >= 0]
    categories = defaultdict(list)
    for event in events:
        categories[event.get('cat', '')].append(event)
    kernels = categories['kernel']
    cpu = categories['cpu_op']
    external = {e.get('args', {}).get('External id'): e['name'] for e in cpu
                if 'External id' in e.get('args', {})}
    attributed = [dict(name=external.get(e.get('args', {}).get('External id'), '<unattributed>'),
                       dur=e['dur']) for e in kernels]
    # Subtract nested CPU operator time once; runtime calls stay inside CPU time.
    threads = defaultdict(list)
    for event in cpu:
        threads[event['pid'], event['tid']].append(event)
    exclusive = []
    indexing_paths = {}
    indexing_shapes = {}
    for thread in threads.values():
        stack = []
        for event in sorted(thread, key=lambda e: (e['ts'], -e['dur'])):
            start, end = event['ts'], event['ts'] + event['dur']
            while stack and stack[-1][0] <= start:
                stack.pop()
            item = dict(name=event['name'], dur=event['dur'])
            if event['name'] == 'aten::_index_put_impl_':
                identity = event.get('args', {}).get('External id')
                indexing_paths[identity] = ' > '.join(
                    [entry[1]['name'] for entry in stack] + [event['name']])
                dims = event.get('args', {}).get('Input Dims', [])
                if dims and 'IndexBackward0' in indexing_paths[identity]:
                    indexing_shapes[identity] = (dims[0], dims[2])
            if stack and end <= stack[-1][0] + 0.01:
                stack[-1][1]['dur'] -= event['dur']
            exclusive.append(item)
            stack.append((end, item))
    durations = sorted(e['dur'] for e in kernels)
    gpu = kernels + categories['gpu_memcpy'] + categories['gpu_memset']
    return dict(
        schema='aic-kineto-summary-v1',
        notes=['Durations are observed under profiling, not unprofiled throughput.',
               'CPU inclusive, CUDA runtime and GPU durations overlap; do not add them.',
               'GPU busy time is interval union, not SM occupancy or compute utilization.',
               'CPU preparation/native worker threads may lack ATen callbacks.'],
        device=trace.get('deviceProperties'),
        event_categories={key: dict(count=len(rows), seconds=sum(e['dur'] for e in rows)/1e6)
                          for key, rows in categories.items()},
        kernel_count=len(kernels),
        kernel_median_us=durations[len(durations)//2] if durations else None,
        kernel_p90_us=durations[int(len(durations)*0.9)] if durations else None,
        kernel_under_10_us=sum(t < 10 for t in durations),
        kernel_busy_seconds=union_us([(e['ts'], e['ts']+e['dur']) for e in kernels])/1e6,
        gpu_busy_seconds=union_us([(e['ts'], e['ts']+e['dur']) for e in gpu])/1e6,
        gpu_span_seconds=(max(e['ts']+e['dur'] for e in gpu)-min(e['ts'] for e in gpu))/1e6 if gpu else 0,
        top_kernels=grouped(kernels),
        kernel_time_by_operator=grouped(attributed),
        indexing_kernel_ancestry=grouped([
            dict(name=indexing_paths[e['args']['External id']], dur=e['dur'])
            for e in kernels if e.get('args', {}).get('External id') in indexing_paths]),
        indexing_backward_destination=grouped([
            dict(name=str(indexing_shapes[e['args']['External id']][0]), dur=e['dur'])
            for e in kernels if e.get('args', {}).get('External id') in indexing_shapes]),
        indexing_backward_gradient_rank=grouped([
            dict(name=str(len(indexing_shapes[e['args']['External id']][1])), dur=e['dur'])
            for e in kernels if e.get('args', {}).get('External id') in indexing_shapes]),
        indexing_backward_shapes=grouped([
            dict(name=str(indexing_shapes[e['args']['External id']]), dur=e['dur'])
            for e in kernels if e.get('args', {}).get('External id') in indexing_shapes]),
        cpu_inclusive=grouped(cpu),
        cpu_self=grouped(exclusive),
        cuda_runtime=grouped(categories['cuda_runtime'], 50),
        copies=grouped(categories['gpu_memcpy']),
    )


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('trace', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    result = summarize(json.loads(args.trace.read_text()))
    args.output.write_text(json.dumps(result, indent=2) + '\n')
