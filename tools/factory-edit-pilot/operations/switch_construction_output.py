"""Resume a paused learner with file-free construction and a new future input bank."""
import argparse
from copy import deepcopy
from datetime import datetime, timezone
from pathlib import Path
import os
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from blueprint_duration_store import exclusive_run
from blueprint_ppo_store import read, reference, save, sha, write
from blueprint_ppo_update import require
from declarative_execution_migrate import parent
from declarative_learning_launch import validate
from declarative_learning_runtime import account_write


CHANGED_SOURCES = {
    'tools/factory-edit-pilot/native_factory_admission.py',
    'target/release/aic-cli',
}
ADDED_SOURCES = {'tools/factory-edit-pilot/test_native_factory_admission.py'}


def switch(output, verification, *, now=None):
    output = Path(output).resolve()
    now = now or datetime.now(timezone.utc)
    with exclusive_run(output):
        value = parent(output)
        old_launch, old_job, old_matrix = value['launch'], value['job'], value['matrix']
        proof = read(verification)
        require(proof['status'] == 'passed' and proof['normal_input_roundtrip']
                and proof['final_output_comparison'], 'construction.switch: completed storage verification required')
        source = proof['source']
        previous = old_launch['source']['files_sha256']
        current = source['files_sha256']
        changed = {key for key in previous if previous[key] != current.get(key)}
        require(changed == CHANGED_SOURCES and set(current) - set(previous) <= ADDED_SOURCES,
                f'construction.switch: source change exceeds output-only scope: {sorted(changed)}')
        require(source['native_sha256'] == old_launch['source']['native_sha256'],
                'construction.switch: native learner changed')
        config = deepcopy(old_launch['config'])
        old_bank = Path(config['training_input']['directory']).resolve()
        new_bank = output / 'training-inputs-final'
        require(not new_bank.exists(), f'construction.switch: future input bank already exists: {new_bank}')
        config['training_input']['directory'] = str(new_bank)
        until = datetime.fromisoformat(old_launch['duration_amendments'][-1]['requested_until_utc'])
        remaining = (until - now).total_seconds()
        require(remaining > 0, 'construction.switch: morning deadline has passed')
        new_seconds = old_job['seconds'] + remaining
        delta = new_seconds - config['duration']['seconds']
        config['duration']['seconds'] = new_seconds
        config['limits']['arm_seconds'] += delta
        config['limits']['matrix_seconds'] += delta
        validate(config)
        job = deepcopy(old_job)
        # Older imported prefix inputs already belong to the original parent bank.
        # Only fill previously implicit locations; never rewrite any input payload.
        prefix_slots = config['training']['first_update'] * config['training']['chunk_actions'] * config['training']['streams'] // config['training']['episode_actions']
        parent_bank = Path(read(Path(job['source']['directory']) / 'launch.json')['config']['training_input']['directory'])
        for key, entry in job['inputs'].items():
            if entry.get('input_directory') is None:
                entry['input_directory'] = str(parent_bank if int(key) < prefix_slots else old_bank)
        archive = output / 'launch-history' / f'{old_matrix["launch_sha256"]}.json'
        archive.parent.mkdir(exist_ok=True)
        if not archive.exists():
            os.link(output / 'launch.json', archive)
        require(sha(archive) == old_matrix['launch_sha256'], 'construction.switch: archived launch changed')
        amendment = dict(schema='construction-output-amendment-v1', prepared_utc=now.isoformat(),
            previous_launch=reference(archive, output), update=job['update'],
            previous_input_bank=str(old_bank), future_input_bank=str(new_bank),
            future_first_slot=job['update'] * config['training']['chunk_actions'] * config['training']['streams'] // config['training']['episode_actions'],
            source_changes=sorted(changed), verification=dict(path=str(Path(verification).resolve()), sha256=sha(verification)),
            requested_until_utc=until.isoformat(), remaining_seconds=remaining,
            scope='Constructor stdout only. Same learner, optimizers, RNG, replay, chosen starts, data rules and evaluation.')
        launch = deepcopy(old_launch)
        launch.update(config=config, source=source, forecast_seconds=new_seconds)
        launch.setdefault('storage_amendments', []).append(amendment)
        staged = output / 'launch-next.json'
        write(staged, launch)
        identity = sha(staged)
        saved = value['saved']
        saved['protocol_sha256'] = identity
        checkpoint = value['owner'] / f'initial/construction-output-{saved["update"]:04d}.pt.gz'
        require(not checkpoint.exists(), f'construction.switch: checkpoint exists: {checkpoint}')
        save(checkpoint, saved)
        account_write(job, checkpoint)
        job['latest_checkpoint'] = reference(checkpoint, value['owner'])
        matrix = deepcopy(old_matrix)
        matrix['launch_sha256'] = identity
        try:
            staged.replace(output / 'launch.json')
            write(value['owner'] / 'state.json', job)
            matrix['jobs'][str(saved['seed'])]['state'] = reference(value['owner'] / 'state.json', output)
            write(output / 'state.json', matrix)
        except BaseException:
            write(output / 'launch.json', old_launch)
            write(value['owner'] / 'state.json', old_job)
            write(output / 'state.json', old_matrix)
            raise
        return dict(status='ready', update=job['update'], requested_until_utc=until.isoformat(),
                    remaining_seconds=remaining, future_input_bank=str(new_bank), checkpoint=job['latest_checkpoint'])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--verification', type=Path, required=True)
    args = parser.parse_args()
    print(switch(args.output, args.verification))


if __name__ == '__main__':
    main()
