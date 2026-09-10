"""Strict checkpoint continuation into a new immutable PPO schedule."""
from pathlib import Path

from ppo_harness_state import require, read, sha, check_sources, checked_path, load_state, restore_checkpoint

PROTECTED_SOURCES = ('full_factory_input_contracts.py', 'full_factory_selection.py', 'ppo_area_model.py',
                     'ppo_spatial_model.py', 'ppo_training_world.py', 'ppo_training_batch.py',
                     'ppo_training_update.py', 'ppo_training_rollout.py')


def prepare_parent(root, spec, config, environment):
    root = Path(root).resolve()
    protocol, state = read(root / 'protocol.json'), load_state(root)
    check_sources(protocol)
    require(state['status'] in ('complete', 'stopped') and len(state['jobs']) == 1,
            'continuation-parent-state', 'parent must be a terminal single-job run', 'prepare', root)
    job, old = state['jobs'][0], protocol['config']
    require(job['completed_batches'] > 0 and config['candidates'] == [job['candidate']] and config['seeds'] == [job['seed']],
            'continuation-job', 'continue one trained parent with the same candidate and seed', 'prepare', root)
    require(environment == protocol['environment'], 'continuation-environment', 'numerical environment differs from parent', 'prepare', root)
    require(config['ppo'] == old['ppo'] and config['execution'] == old['execution'] and
            config['training']['horizon'] == old['training']['horizon'] and
            spec.get('model') == protocol['spec'].get('model') and spec['policy'] == protocol['spec']['policy'] and
            spec['selection'] == protocol['spec']['selection'],
            'continuation-contract', 'model, execution, PPO or selection differs from parent', 'prepare', root)
    for key in ('generation', 'products'):
        # Product source paths may relocate; compare source identities separately.
        before, after = protocol['spec'][key], spec[key]
        if key == 'products':
            before = [dict(p, source=sha(p['source'])) for p in before]
            after = [dict(p, source=sha(p['source'])) for p in after]
        require(before == after, 'continuation-generation', f'factory generation differs: {key}', 'prepare', root)
    for key in ('native_library', 'cli', 'validator'):
        require(sha(spec[key]) == sha(protocol['spec'][key]), 'continuation-binary', f'binary differs: {key}', 'prepare', spec[key])
    require(set(spec['catalogs']) == set(protocol['spec']['catalogs']) and
            all(sha(path) == sha(protocol['spec']['catalogs'][key]) for key, path in spec['catalogs'].items()),
            'continuation-catalogs', 'catalog contents differ from parent', 'prepare', root)
    for name in PROTECTED_SOURCES:
        expected = [h for path,h in protocol['sources'].items() if Path(path).name == name]
        require(len(expected) == 1 and sha(Path(__file__).with_name(name)) == expected[0],
                'continuation-source', f'model/execution source differs: {name}', 'prepare', root)
    require(config.get('input_manifest_scope') == 'evaluation', 'continuation-input-scope',
            'continuation requires fresh training and explicitly frozen evaluation', 'prepare', root)
    parent_eval = {old['evaluation']['generation_seed_base'] + job['seed']*100000 + i for i in range(old['evaluation']['attempts'])}
    new_eval = set(spec['frozen_starts']['evaluation_seeds'])
    require(parent_eval == new_eval and config['evaluation']['action_seed_base'] == old['evaluation']['action_seed_base'],
            'continuation-evaluation', 'evaluation factory/action seed cohort differs from parent', 'prepare', root)
    for seed in parent_eval:
        key = str(seed)
        before = protocol['spec']['frozen_starts']['entries'].get(key)
        after = spec['frozen_starts']['entries'].get(key)
        require(before is not None and after is not None and before['status'] == after['status'] and
                before['files'] == after['files'], 'continuation-evaluation-input', 'frozen evaluation identity differs', 'prepare', seed)
    # New run-local batch 1 starts after every training seed consumed by the parent.
    # Also charge retained partial-phase attempts after a normal duration stop.
    attempts = max([job['completed_batches'] * old['training']['attempts']] +
                   [r['phase']['batch'] * old['training']['attempts'] for r in job['records'] if r['phase']['kind'] == 'training'])
    for path in root.glob(f"{job['id']}/training-*/episode-*/logical-task.json"):
        task = read(path)['identity']
        attempts = max(attempts, (task['batch'] - 1) * old['training']['attempts'] + task['index'] + 1)
    for key in ('generation_seed_base', 'action_seed_base'):
        require(config['training'][key] >= old['training'][key] + attempts,
                'continuation-training-overlap', f'new training {key} overlaps consumed parent attempts', 'prepare', root)
    checkpoint = checked_path(root, job['checkpoint'])
    ancestor = protocol.get('continuation', {})
    lineage = dict(schema='ppo-checkpoint-continuation-v1', root=str(root),
                   protocol_sha256=state['protocol_sha256'], state_sha256=sha(root/'state.json'),
                   job=job['id'], checkpoint=job['checkpoint'], batch=job['completed_batches'],
                   total_batches=ancestor.get('total_batches', 0) + job['completed_batches'],
                   total_training_actions=ancestor.get('total_training_actions', 0) + job['selected_training_actions'])
    return lineage, {root/'protocol.json', root/'state.json', checkpoint}


def restore_parent(parent, model_set):
    root = Path(parent['root'])
    require(sha(root/'protocol.json') == parent['protocol_sha256'] and sha(root/'state.json') == parent['state_sha256'],
            'continuation-parent-drift', 'parent protocol/state changed after preparation', 'initialization', root)
    saved = restore_checkpoint(root, parent['checkpoint'], model_set, parent['protocol_sha256'], parent['job'], parent['batch'])
    require(saved['actor_optimizer']['state'] and saved['critic_optimizer']['state'],
            'continuation-empty-adam', 'trained parent must contain both optimizer states', 'initialization', root)
    return saved
