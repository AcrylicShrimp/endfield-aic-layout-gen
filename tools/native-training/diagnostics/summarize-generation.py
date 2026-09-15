"""Summarize isolated generation phase records; overlapping worker time is explicit."""
import argparse
from collections import defaultdict
import json
from pathlib import Path
import statistics


def summarize(log):
    records = [json.loads(line) for line in log.splitlines() if line.startswith('{')]
    slots = [r for r in records if r.get('schema') == 'aic-generation-phase-profile-v1']
    processes = [r for r in records if r.get('schema') == 'aic-generation-process-profile-v1']
    if not slots or not all(r['success'] for r in slots):
        raise ValueError('Complete successful generation records required')
    if len({r['slot'] for r in slots}) != len(slots):
        raise ValueError('Duplicate canonical generation slots')
    workers = defaultdict(list)
    phases = defaultdict(float)
    for row in slots:
        workers[row['thread']].append(row)
        for phase, seconds in row['phases'].items():
            phases[phase] += seconds
    first = min(r['start_us'] for r in slots)
    worker_rows = []
    for thread, rows in workers.items():
        last = max(r['start_us'] + r['seconds'] * 1e6 for r in rows)
        totals = defaultdict(float)
        for row in rows:
            for phase, seconds in row['phases'].items():
                totals[phase] += seconds
        worker_rows.append(dict(thread=thread, slots=[r['slot'] for r in rows],
                                generation_seconds=sum(r['seconds'] for r in rows),
                                finish_after_first_start_seconds=(last-first)/1e6,
                                phases=dict(totals)))
    command_rows = []
    commands = defaultdict(list)
    for row in processes:
        if len(row['child']) != 1:
            raise ValueError('Each child must have one main timer; preload missing?')
        commands[row['command']].append(row)
    for command, rows in commands.items():
        parent = sum(r['seconds'] for r in rows)
        main = sum(r['child'][0]['main_seconds'] for r in rows)
        command_rows.append(dict(command=command, count=len(rows),
            failed_processes=sum(not r['success'] for r in rows),
            failed_process_wall_seconds=sum(r['seconds'] for r in rows if not r['success']),
            process_wall_seconds=parent,
            spawn_seconds=sum(r['spawn_seconds'] for r in rows),
            child_main_wall_seconds=main,
            outside_child_main_wall_seconds=parent-main,
            child_main_cpu_seconds=sum(r['child'][0]['main_cpu_seconds'] for r in rows),
            child_before_main_cpu_seconds=sum(r['child'][0]['total_cpu_seconds']-
                r['child'][0]['main_cpu_seconds'] for r in rows)))
    durations = sorted(r['seconds'] for r in slots)
    return dict(schema='aic-generation-profile-summary-v1',
        notes=['Phase/command sums are overlapping worker-seconds, not elapsed training time.',
               'Outside child main includes startup, shutdown, parent polling and output handling.',
               'Child before-main CPU measures dynamic initialization, not parent waiting.',
               'Worker finish spans include gaps between Source::generate calls.'],
        slots=len(slots), workers=len(workers), processes=len(processes),
        generation_worker_seconds=sum(durations),
        slot_median_seconds=statistics.median(durations), slot_max_seconds=max(durations),
        generation_span_seconds=max(r['finish_after_first_start_seconds'] for r in worker_rows),
        phases_worker_seconds=dict(phases), commands=command_rows,
        workers_by_finish=sorted(worker_rows,key=lambda r:-r['finish_after_first_start_seconds']),
        slowest_slots=sorted(slots,key=lambda r:-r['seconds'])[:8])


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('log', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    args.output.write_text(json.dumps(summarize(args.log.read_text()),indent=2)+'\n')
