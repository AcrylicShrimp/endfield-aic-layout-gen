"""Explicit post-training autonomous continuation job; never an automatic tail hook."""
import json
import os
from pathlib import Path
import time

from blueprint_edit_input import BlueprintActorCritic
from blueprint_execution_pool import BlueprintPool
from blueprint_execution_resources import ResourceGuard
from blueprint_ppo_store import checked, load, read, reference, save, sha, write
from blueprint_replay_contract import Sources, check
from blueprint_replay_launch import check_launch
from blueprint_replay_runtime import admitted_sources, child_config


def run_probe(matrix, output):
    matrix, output = Path(matrix).resolve(), Path(output).resolve()
    launch, state = read(matrix / 'launch.json'), read(matrix / 'state.json')
    check(state['status'] == 'complete' and state['launch_sha256'] == sha(matrix / 'launch.json'),
        'source-invalid', 'continuation', 'main matrix must be complete and bound before the separate probe')
    check(not output.exists(), 'source-invalid', 'continuation', 'fresh probe output required', output)
    check_launch(launch, Sources())
    output.mkdir(parents=True)
    spec, allocation = launch['spec'], launch['allocation']
    options = spec['anchor_continuation']
    guard = ResourceGuard(output, min(3600, spec['limits']['matrix_seconds'] - state['seconds']),
        spec['limits']['matrix_rss_bytes'], spec['limits']['min_free_bytes'])
    parent, imports, _ = admitted_sources(launch['source_directory'], 17)
    config = child_config(parent, spec, launch['training_input_directory'])
    model = BlueprintActorCritic(spec['ppo']['width'], spec['ppo']['message_rounds'])
    records = []
    pool = None
    try:
        pool = BlueprintPool(config, launch['native_library'], allocation['workers'], output / 'workers', guard,
            saved=parent['learner']['streams'])
        os.sched_setaffinity(0, allocation['coordinator'])
        for seed in spec['seeds']:
            for update in options['updates']:
                names = ['C'] if update == 0 else spec['order'][str(seed)]
                for arm in names:
                    checkpoint = matrix / f'{arm}-seed-{seed}/checkpoints/{update:04d}.pt.gz'
                    saved = load(checkpoint)
                    check(saved['protocol_sha256'] == state['launch_sha256'] and saved['child_update'] == update,
                        'source-invalid', 'continuation', 'checkpoint identity differs', checkpoint)
                    model.load_state_dict(saved['model'], strict=True)
                    model.eval()
                    pool.exchange([dict(operation='model', state=model.state_dict()) for _ in pool.workers])
                    directory = output / f'{arm}-seed-{seed}' / f'{update:04d}'
                    commands = []
                    for case_index, sequence in enumerate(imports):
                        for anchor in sequence['anchor_states']:
                            stage_index = ('pose', 'repair', 'commit').index(anchor['stage'])
                            for replicate in range(options['replicates']):
                                commands.append(dict(operation='continuation', state=anchor, case_index=case_index,
                                    replicate=replicate, branch_seconds=options['branch_seconds'],
                                    action_seed=options['action_seed_base'] + 1000 * case_index + 100 * stage_index + replicate,
                                    output=str(directory), deadline=guard.began + guard.seconds))
                    rows = []
                    for start in range(0, len(commands), len(pool.workers)):
                        rows.extend(r['row'] for r in pool.exchange(commands[start:start + len(pool.workers)]))
                    record = dict(seed=seed, arm=arm, update=update, shared_across_arms=update == 0,
                        checkpoint=dict(path=str(checkpoint), sha256=sha(checkpoint)), rows=rows)
                    write(directory / 'report.json', record)
                    records.append(reference(directory / 'report.json', output))
                    write(output / 'progress.json', dict(records=records))
                    print(json.dumps(dict(stage='anchor-continuation', seed=seed, arm=arm, update=update,
                        branches=len(rows), target_attained=sum(r['demonstrated_target_attained'] for r in rows))), flush=True)
        report = dict(schema='blueprint-replay-continuation-v1', status='complete', launch_sha256=state['launch_sha256'],
            options=options, records=records, seconds=time.monotonic() - guard.began, resource=guard.last)
        write(output / 'report.json', report)
        state['continuation'] = dict(path=str(output / 'report.json'), sha256=sha(output / 'report.json'))
        state['seconds'] += report['seconds']
        write(matrix / 'state.json', state)
        return report
    finally:
        if pool:
            pool.close()
        os.sched_setaffinity(0, allocation['allowed'])
