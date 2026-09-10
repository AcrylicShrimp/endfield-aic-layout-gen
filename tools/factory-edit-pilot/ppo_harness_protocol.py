"""Freeze an explicit fresh-factory PPO schedule and its runtime dependencies."""
import ast
from copy import deepcopy
import math
import importlib.metadata
import platform
from pathlib import Path
import random

from native_factory_runtime import ROOT, BACKEND
from full_factory_input_contracts import CONTRACTS
from ppo_harness_environment import tracking_destination
from ppo_harness_state import HarnessError, sync_directory, SCHEMA, require, read, write, sha, mkdir
from random_factory_generation import resolve_spec


def fields(value, expected, name):
    require(isinstance(value, dict) and set(value) == set(expected.split()), 'config-fields',
            f'{name} requires exactly: {expected}', 'prepare', name)


def integer(value, low, high, name):
    require(type(value) is int and low <= value <= high, 'config-range',
            f'{name} must be an integer in [{low}, {high}]', 'prepare', name)


def positive(value, maximum, name):
    require(type(value) in (int, float) and math.isfinite(value) and 0 < value <= maximum,
            'config-range', f'{name} must be finite in (0, {maximum}]', 'prepare', name)


def validate(config):
    fields(config, 'schema_version purpose generation_config candidates seeds order_seed training evaluation execution limits ppo tracking retention'
           + (' input_manifest' if 'input_manifest' in config else '')
           + (' input_manifest_scope' if 'input_manifest_scope' in config else '')
           + (' input_manifest_training_batches' if 'input_manifest_training_batches' in config else '')
           + (' reward' if 'reward' in config else '')
           + (' start_curriculum' if 'start_curriculum' in config else '')
           + (' continuation_run' if 'continuation_run' in config else '')
           + (' model' if 'model' in config else ''), 'config')
    if 'reward' in config:
        from ppo_reward import validate as validate_reward
        validate_reward(config['reward'])
    if 'start_curriculum' in config:
        from ppo_start_curriculum import validate as validate_curriculum
        validate_curriculum(config['start_curriculum'])
        require('continuation_run' not in config, 'curriculum-continuation',
                'prepare curriculum runs from fresh models; resume an existing run through its own state', 'prepare')
    if 'input_manifest_scope' in config:
        require('input_manifest' in config and config['input_manifest_scope'] in ('all', 'evaluation', 'prefix-and-evaluation'),
                'config-input-scope', 'input_manifest_scope requires a manifest and a registered scope', 'prepare')
    if 'input_manifest_training_batches' in config or config.get('input_manifest_scope') == 'prefix-and-evaluation':
        require(config.get('input_manifest_scope') == 'prefix-and-evaluation', 'config-input-prefix',
                'training prefix requires prefix-and-evaluation scope', 'prepare')
        integer(config.get('input_manifest_training_batches'), 1, config['training']['batches'], 'input_manifest_training_batches')
    if 'continuation_run' in config:
        require(isinstance(config['continuation_run'], str) and bool(config['continuation_run']),
                'config-continuation', 'continuation_run must name an immutable parent run', 'prepare')
    if 'model' in config:
        from ppo_spatial_model import validate_model
        validate_model(config['model'])
    if 'input_manifest' in config:
        require(isinstance(config['input_manifest'], str) and config['input_manifest'], 'config-input-manifest',
                'input_manifest must be an explicit path to frozen initial factories', 'prepare')
    require(config['schema_version'] == 1, 'config-schema', 'supported schema_version is 1', 'prepare')
    require(config['purpose'] in ('readiness', 'input-screen'), 'config-purpose', 'readiness or input-screen purpose required')
    require(isinstance(config['generation_config'], str) and config['generation_config'], 'config-generation', 'generation_config path required')
    candidates, seeds = config['candidates'], config['seeds']
    require(isinstance(candidates, list) and candidates and all(c in CONTRACTS for c in candidates) and
            len(set(candidates)) == len(candidates), 'config-candidates', 'unique registered input contracts required')
    require(isinstance(seeds, list) and seeds and len(seeds) == len(set(seeds)), 'config-seeds', 'unique initialization seeds required')
    for seed in seeds:
        integer(seed, 0, 9999, 'seed')
    integer(config['order_seed'], 0, 2**32 - 1, 'order_seed')
    train, evaluation, execution, limits, ppo = (config[k] for k in ('training', 'evaluation', 'execution', 'limits', 'ppo'))
    fields(train, 'batches attempts horizon generation_seed_base action_seed_base', 'training')
    integer(train['batches'], 1, 16384, 'training.batches')
    integer(train['attempts'], 2, 256, 'training.attempts')
    integer(train['horizon'], 1, 256, 'training.horizon')
    fields(config['retention'], 'training_batches', 'retention')
    retained = config['retention']['training_batches']
    require(isinstance(retained, list) and len(retained) == len(set(retained)) and
            all(type(n) is int and 1 <= n <= train['batches'] for n in retained),
            'config-retention', 'retained training batches must be unique scheduled batch numbers')
    fields(evaluation, 'at attempts generation_seed_base action_seed_base', 'evaluation')
    integer(evaluation['attempts'], 2, 256, 'evaluation.attempts')
    at = evaluation['at']
    require(isinstance(at, list) and at and all(type(n) is int for n in at) and at == sorted(set(at)) and
            at[0] == 0 and at[-1] == train['batches'], 'config-evaluation', 'evaluation at must include 0 and final batch in increasing order')
    for schedule in (train, evaluation):
        for name in ('generation_seed_base', 'action_seed_base'):
            integer(schedule[name], 0, 2**48 - 1, name)
    # Explicit interval disjointness includes every initialization seed and task.
    for name in ('generation_seed_base', 'action_seed_base'):
        training = {train[name] + seed * 100000 + i for seed in seeds for i in range(train['batches'] * train['attempts'])}
        held_out = {evaluation[name] + seed * 100000 + i for seed in seeds for i in range(evaluation['attempts'])}
        require(not training & held_out, 'config-seed-overlap', f'training/evaluation overlap: {name}')
    fields(execution, 'workers device microbatch', 'execution')
    integer(execution['workers'], 1, 64, 'execution.workers')
    require(execution['device'] in ('cpu', 'cuda'), 'config-device', 'explicit cpu or cuda required')
    fields(limits, 'startup_seconds task_seconds batch_seconds progress_seconds run_seconds rss_bytes disk_reserve_bytes cleanup_seconds', 'limits')
    for name in ('startup_seconds', 'task_seconds', 'batch_seconds', 'progress_seconds', 'run_seconds', 'cleanup_seconds'):
        positive(limits[name], 86400, 'limits.' + name)
    require(limits['task_seconds'] <= limits['batch_seconds'] and limits['progress_seconds'] >= limits['task_seconds'],
            'config-timeouts', 'batch/progress limits must cover one task')
    integer(limits['rss_bytes'], 1024**2, 1024**4, 'limits.rss_bytes')
    integer(limits['disk_reserve_bytes'], 0, 1024**4, 'limits.disk_reserve_bytes')
    fields(ppo, 'gamma gae_lambda clip_epsilon epochs minibatch_size target_kl learning_rate entropy_coefficient gradient_clip', 'ppo')
    require(ppo['gamma'] == 1 and type(ppo['gae_lambda']) in (int, float) and 0 <= ppo['gae_lambda'] <= 1,
            'config-credit', 'gamma=1 and GAE lambda in [0,1] required')
    for name in ('clip_epsilon', 'target_kl', 'learning_rate', 'gradient_clip'):
        positive(ppo[name], 1, 'ppo.' + name)
    require(type(ppo['entropy_coefficient']) in (int, float) and 0 <= ppo['entropy_coefficient'] <= 1,
            'config-entropy', 'entropy coefficient in [0,1] required')
    integer(ppo['epochs'], 1, 16, 'ppo.epochs')
    integer(ppo['minibatch_size'], 1, 1024, 'ppo.minibatch_size')
    integer(execution['microbatch'], 1, ppo['minibatch_size'], 'execution.microbatch')
    fields(config['tracking'], 'backend mode entity project', 'tracking')
    require(config['tracking']['backend'] in ('none', 'wandb') and
            config['tracking']['mode'] in ('offline', 'online'), 'config-tracking', 'explicit tracking backend/mode required')
    require(all(isinstance(config['tracking'][k], str) for k in ('entity', 'project')), 'config-tracking', 'tracking entity/project must be strings')


def sources():
    directory = Path(__file__).parent
    pending = [directory / name for name in ('ppo_harness.py', 'ppo_harness_run.py', 'ppo_harness_worker.py',
                                             'ppo_harness_monitor.py', 'ppo_harness_tracking.py')]
    found = set()
    while pending:
        path = pending.pop().resolve()
        if path in found:
            continue
        found.add(path)
        for node in ast.walk(ast.parse(path.read_text())):
            names = ([node.module] if isinstance(node, ast.ImportFrom) else
                     [alias.name for alias in node.names] if isinstance(node, ast.Import) else [])
            for name in names:
                local = directory / f'{name}.py'
                if local.is_file():
                    pending.append(local)
    return found


def validate_spec(spec):
    policy, selection, generation = spec['policy'], spec['selection'], spec['generation']
    require(isinstance(spec['products'], list) and spec['products'], 'config-products', 'nonempty product list required')
    for product in spec['products']:
        fields(product, 'id source duration_ms', 'product')
        bounds = product['duration_ms']
        require(isinstance(bounds, list) and len(bounds) == 2 and all(type(n) is int for n in bounds) and
                0 < bounds[0] <= bounds[1] <= 10**9, 'config-product-duration', 'product duration requires two positive integer bounds', entity=product.get('id'))
    fields(policy, 'depth width', 'policy')
    integer(policy['depth'], 1, 16, 'policy.depth')
    integer(policy['width'], 1, 1024, 'policy.width')
    fields(selection, 'horizon episode_seconds route_work memory balanced temperature', 'selection')
    integer(selection['horizon'], 1, 256, 'selection.horizon')
    positive(selection['episode_seconds'], 3600, 'selection.episode_seconds')
    positive(selection['temperature'], 100, 'selection.temperature')
    integer(selection['route_work'], 1, 10**8, 'selection.route_work')
    require(type(selection['memory']) is bool and type(selection['balanced']) is bool,
            'config-selection', 'memory and balanced must be booleans')
    fields(generation, 'flow_elapsed_ms validator_ms max_facilities duration_step_ms construction setup_steps setup_seconds setup_options setup_kinds', 'generation')
    for key in ('flow_elapsed_ms', 'validator_ms', 'duration_step_ms'):
        integer(generation[key], 1, 10**9, 'generation.' + key)
    integer(generation['max_facilities'], 1, 10**4, 'generation.max_facilities')
    positive(generation['setup_seconds'], 3600, 'generation.setup_seconds')
    fields(generation['construction'], 'elapsed_ms max_steps max_placement_attempts max_route_searches analysis_workers', 'generation.construction')
    for key, value in generation['construction'].items():
        integer(value, 1, 10**9 if key != 'analysis_workers' else 64, 'generation.construction.' + key)
    fields(generation['setup_options'], 'pose_domain mode', 'generation.setup_options')
    require(generation['setup_options'] == dict(pose_domain='basic', mode='local'),
            'config-setup-options', 'this fresh-factory generator requires basic/local setup options')
    from full_factory_policy import KINDS
    kinds = generation['setup_kinds']
    require(isinstance(kinds, list) and kinds and all(k in KINDS for k in kinds) and len(set(kinds)) == len(kinds),
            'config-setup-kinds', 'unique registered setup kinds required')
    steps = generation['setup_steps']
    require(isinstance(steps, list) and len(steps) == 2 and all(type(s) is int for s in steps) and
            1 <= steps[0] <= steps[1] <= 32, 'config-setup-steps', 'setup steps require an integer interval within [1,32]')


def phases(config):
    result = []
    for batch in range(config['training']['batches'] + 1):
        if batch:
            result.append(dict(kind='training', batch=batch))
        if batch in config['evaluation']['at']:
            result.append(dict(kind='evaluation', batch=batch))
    return result


def environment():
    import torch
    packages = {}
    for name in ('torch', 'numpy', 'wandb'):
        try:
            packages[name] = importlib.metadata.version(name)
        except importlib.metadata.PackageNotFoundError:
            packages[name] = None
    return dict(python=platform.python_version(), platform=platform.system(), packages=packages,
                cuda=torch.version.cuda)


def prepare(config_path, root):
    config_path, root = Path(config_path).resolve(), Path(root).resolve()
    config = read(config_path)
    validate(config)
    from ppo_reward import DEFAULT
    config.setdefault('reward', dict(DEFAULT))
    if config['tracking']['backend'] == 'wandb':
        destination = tracking_destination(required=config['tracking']['mode'] == 'online')
        for key, value in destination.items():
            if value:
                config['tracking'][key] = value
        require(bool(config['tracking']['project']), 'tracking-project', 'WANDB_PROJECT or tracking.project required', 'prepare')
    require(not root.exists(), 'existing-run', f'prepare requires a fresh directory: {root}', 'prepare', root)
    generation_config = (ROOT / config['generation_config']).resolve()
    spec = deepcopy(resolve_spec(generation_config))
    spec = {k: spec[k] for k in ('native_library', 'cli', 'validator', 'catalogs', 'products', 'generation', 'policy', 'selection')}
    spec['policy'] = {k: spec['policy'][k] for k in ('depth', 'width')}
    if 'model' in config:
        spec['model'] = deepcopy(config['model'])
    spec['selection']['horizon'] = config['training']['horizon']
    validate_spec(spec)
    jobs = [dict(candidate=c, seed=s, id=f'{c}--seed-{s}') for c in config['candidates'] for s in config['seeds']]
    random.Random(config['order_seed']).shuffle(jobs)
    paths = sources() | {config_path, generation_config}
    if 'input_manifest' in config:
        from ppo_frozen_starts import load
        spec['frozen_starts'], frozen_files = load(ROOT / config['input_manifest'], config,
                                                  config.get('input_manifest_scope', 'all'))
        paths.update(frozen_files)
    continuation = None
    if 'continuation_run' in config:
        from ppo_harness_continuation import prepare_parent
        continuation, parent_files = prepare_parent(ROOT / config['continuation_run'], spec, config, environment())
        paths.update(parent_files)
    paths.update(Path(__file__).with_name(name) for name in ('pyproject.toml', 'uv.lock'))
    paths.update(Path(spec[k]) for k in ('native_library', 'cli', 'validator'))
    paths.update(map(Path, spec['catalogs'].values()))
    paths.update(Path(product['source']) for product in spec['products'])
    paths.update(ROOT.glob('crates/aic-data/src/**/*.rs'))
    paths.update(ROOT.glob('crates/aic-pilot-native/src/**/*.rs'))
    paths.update(ROOT / name for name in ('Cargo.toml', 'Cargo.lock', 'crates/aic-data/Cargo.toml',
                                        'crates/aic-pilot-native/Cargo.toml', 'crates/aic-pilot-native/build.rs'))
    identities = {str(p.resolve()): sha(p) for p in sorted(paths)}
    mkdir(root.parent)
    try:
        root.mkdir()
    except FileExistsError as error:
        raise HarnessError('existing-run', f'prepare requires a fresh directory: {root}', 'prepare', root) from error
    sync_directory(root.parent)
    write(root / 'catalogs.json', spec['catalogs'])
    identities[str(root / 'catalogs.json')] = sha(root / 'catalogs.json')
    protocol = dict(schema=SCHEMA, backend=BACKEND, config=config, spec=spec, jobs=jobs, phases=phases(config), environment=environment(),
                    catalog_file=str(root / 'catalogs.json'), sources=identities)
    if continuation is not None:
        protocol['continuation'] = continuation
    write(root / 'protocol.json', protocol)
    write(root / 'state.json', dict(schema=SCHEMA, protocol_sha256=sha(root / 'protocol.json'), status='prepared',
                                  jobs=[], cursor=0, elapsed_seconds=0., sessions=0))
    return protocol
