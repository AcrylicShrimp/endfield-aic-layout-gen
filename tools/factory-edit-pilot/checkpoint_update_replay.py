"""Restore one historical PPO interval and measure optimizer/calibration movement."""

import argparse
from copy import deepcopy
import json
from pathlib import Path
import random
import subprocess
import time

import numpy as np
import torch

from case_curriculum import CaseCurriculum, assess_cases
from case_curriculum_audit import replay_assessment
from case_curriculum_study import load
from curriculum import Collector, FIELDS
from evidence import append, write
from mastery_policy_selection import digest, read
from native import layout, snapshot, trace
from native_training_comparison import equal, policy_hash, rows
from policy import Policy
from prefix_recovery import replay
from reference.acceptance import Catalog
from run import refresh_actor
from two_case import study_provenance
from update_stability import measured_update


def require_protocol(spec):
    if (spec.get('schema_version') != 1 or spec.get('purpose') != 'checkpoint_update_replay'
            or spec['start_commands'] != 1179648 or spec['end_commands'] != 1212416
            or spec['updates'] != 32 or spec['assessment_stride'] != 4
            or spec['device'] != 'cuda' or spec['seed'] != 43
            or spec['condition'] != 'entity-mastery-rehearsal'
            or spec['historical_sources'] != ['policy.py', 'curriculum.py', 'case_curriculum.py',
                'evaluation_batch.py', 'case_curriculum_study.py', 'native.py', 'run.py']):
        raise ValueError('checkpoint_replay.protocol: unsupported historical interval')


def episode_record(case, env, collected, episode, commands, remaining, reason, current_level, prefix, horizon):
    counts = json.loads(env.episode_json())
    total = sum(collected['rewards'])
    if abs(total-(env.initial_score[0]-env.best_score[0])/env.initial_score[0]) > 1e-10:
        raise ValueError('checkpoint_replay.accounting: collection reward differs from incumbent')
    return {'case': case, 'episode': episode, 'commands': commands, 'remaining': remaining,
        'horizon': horizon, 'setup_commands': len(prefix), 'current_level': current_level,
        'start_reason': reason, 'original_root': not prefix, 'return': total,
        'score': list(env.best_score), 'counts': counts['counts'], 'unfinished': not counts['draft_acceptance']['valid']}


def cpu_state(value):
    if isinstance(value, torch.Tensor):
        return value.detach().cpu()
    if isinstance(value, dict):
        return {key: cpu_state(item) for key, item in value.items()}
    if isinstance(value, list):
        return [cpu_state(item) for item in value]
    if isinstance(value, tuple):
        return tuple(cpu_state(item) for item in value)
    return value


def compare_record(actual, expected):
    return {key: {'actual': actual.get(key), 'expected': expected.get(key)}
            for key in actual.keys() | expected.keys() if not equal(actual.get(key), expected.get(key))}


def run(root, source, protocol_path, library, output):
    spec = read(protocol_path)
    require_protocol(spec)
    if output.exists():
        raise ValueError(f'checkpoint_replay.output: {output}: choose a fresh directory')
    if subprocess.check_output(['git', 'status', '--porcelain'], cwd=root, text=True).strip():
        raise ValueError('checkpoint_replay.source: require clean diagnostic checkout')
    audit, metadata = read(source/'study-audit.json'), read(source/'metadata.json')
    if (audit['status'] != 'passed' or metadata['status'] != 'complete'
            or audit['metadata_sha256'] != digest(source/'metadata.json')
            or audit['source_commit'] != spec['training_source_commit']
            or metadata['seed'] != spec['seed'] or metadata['config']['width_multiplier'] != 1):
        raise ValueError('checkpoint_replay.audit: source is not the declared completed audited stream')
    checkpoint_paths = [source/f'checkpoint-{spec[key]}.pt' for key in ('start_commands', 'end_commands')]
    checkpoint_hashes = [digest(path) for path in checkpoint_paths]
    if any(actual != audit['checkpoints_sha256'][str(spec[key])]
           for actual, key in zip(checkpoint_hashes, ('start_commands', 'end_commands'))):
        raise ValueError('checkpoint_replay.checkpoint: checkpoint differs from independent audit')
    start, end = [torch.load(path, map_location='cpu', weights_only=False) for path in checkpoint_paths]
    if (any(saved['kind'] != 'complete_rollout' or saved['metadata']['completed_commands'] != spec[key]
            for saved, key in zip((start, end), ('start_commands', 'end_commands')))
            or not equal(start['policy'], start['evaluation_policy'])):
        raise ValueError('checkpoint_replay.checkpoint: incompatible completed boundary')
    for name in spec['historical_sources']:
        relative = f'tools/factory-edit-pilot/{name}'
        if digest(root/relative) != next(iter(metadata['provenance'].values()))['files_sha256'][relative]:
            raise ValueError(f'checkpoint_replay.source: historical source differs: {relative}')
    original_protocol, base, config, training, groups = load(root, root/spec['study_protocol'],
                                                           spec['condition'], spec['seed'], library)
    runtimes, training_runtimes, assessment_runtimes = groups
    if (config != start['metadata']['config'] or training != start['metadata']['training_config']
            or original_protocol != metadata['protocol'] or training['rollout_commands'] != 1024
            or training['horizon'] != 16 or original_protocol['streams'] != 16
            or original_protocol['assessment']['horizon'] != 16 or original_protocol['assessment']['episodes'] != 64
            or spec['end_commands']-spec['start_commands'] != spec['updates']*1024):
        raise ValueError('checkpoint_replay.contract: restored exposure differs from historical training')
    torch.set_num_threads(config['threads'])
    torch.backends.cuda.matmul.allow_tf32 = False
    torch.backends.cudnn.allow_tf32 = False
    torch.backends.cudnn.benchmark = False
    provenance = {}
    for key, runtime in runtimes.items():
        current = study_provenance(runtime, root/spec['study_protocol'], base)
        previous = metadata['provenance'][key]
        if (any(current[field] != previous[field] for field in ('python', 'torch', 'numpy', 'platform', 'machine'))
                or current['native'] != {**previous['native'], 'library': current['native']['library']}
                or digest(runtime.path) != previous['files_sha256'][str(runtime.path.relative_to(root))]):
            raise ValueError(f'checkpoint_replay.runtime: runtime/native/input identity differs: {key}')
        provenance[key] = current
    history_episodes = [row for row in rows(source/'episodes.jsonl')
                        if spec['start_commands'] < row['commands'] <= spec['end_commands']]
    history_updates = [row for row in rows(source/'updates.jsonl')
                       if spec['start_commands'] < row['commands'] <= spec['end_commands']]
    history_gates = {row['commands']: row for row in rows(source/'assessments.jsonl')
                     if row['commands'] in (spec['start_commands'], spec['end_commands'])}
    if (len(history_episodes) != 2048 or len(history_updates) != 32 or len(history_gates) != 2
            or [row['commands'] for row in history_updates] != list(range(spec['start_commands']+1024, spec['end_commands']+1, 1024))):
        raise ValueError('checkpoint_replay.history: incomplete interval evidence')
    output.mkdir(parents=True)
    result = {'schema_version': 1, 'status': 'running', 'protocol': spec, 'protocol_sha256': digest(protocol_path),
        'checkpoint_sha256': checkpoint_hashes, 'audit_sha256': digest(source/'study-audit.json'),
        'historical_record_sha256': {name: digest(source/name) for name in ('episodes.jsonl', 'updates.jsonl', 'assessments.jsonl')},
        'provenance': provenance, 'updates': [], 'assessments': [], 'matched_episodes': 0,
        'first_episode_mismatch': None, 'first_metric_mismatch': None}
    model, collector, optimizer, actor, rng, controller = None, None, None, None, None, None
    commands, completed_episodes, active = spec['start_commands'], start['metadata']['episodes'], 'setup'
    started = time.perf_counter()

    def save_state(path, complete):
        torch.save({'schema_version': 1, 'purpose': 'historical_replay_diagnostic', 'commands': commands,
            'episodes': completed_episodes, 'phase': active, 'config': config, 'training_config': training,
            'controller': controller.state, 'policy': model.state_dict(), 'evaluation_policy': actor.state_dict(),
            'optimizer': optimizer.state_dict(), 'torch_rng': torch.get_rng_state(),
            'numpy_rng': rng.bit_generator.state, 'numpy_legacy_rng': np.random.get_state(), 'python_rng': random.getstate(),
            'cuda_rng': torch.cuda.get_rng_state_all(), 'stream_rngs': [g.get_state() for g in collector.generators],
            'environments': [snapshot(env) for env in collector.envs],
            'partial_collection': None if complete else collector.partial,
            'pending_transition': None if complete else collector.pending}, path)

    def assess(update_index):
        levels = {key: value['level'] for key, value in controller.state.items()}
        before = policy_hash(actor.state_dict())
        gate = assess_cases(actor, assessment_runtimes, original_protocol, levels, config)
        if policy_hash(actor.state_dict()) != before:
            raise ValueError('checkpoint_replay.assessment: assessment changed actor')
        path = output/f'assessment-{update_index}.json'
        write(path, gate)
        historical = history_gates.get(commands)
        # Anchor is assessed at its restored levels, which may differ from pre-transition levels.
        comparison = None if historical is None else {key: equal(gate[key], historical[key])
                                                      for key in ('policy_sha256', 'level', 'successes', 'rows')}
        result['assessments'].append({'update': update_index, 'commands': commands, 'successes': gate['successes'],
            'historical_fields_equal': comparison, 'file': path.name, 'sha256': digest(path)})
        write(output/'replay.json', result)
        print(json.dumps({'event': 'checkpoint_replay_assessment', 'update': update_index,
                          'successes': sum(gate['successes'].values()), 'weakest': min(gate['successes'].values())}), flush=True)
        return gate

    try:
        example, _ = next(iter(runtimes.values())).new_engine().observe()
        model = Policy(example, normalization=config['normalization'], width_multiplier=1).to(config['device'])
        model.load_state_dict(start['policy'])
        actor = deepcopy(model).to(config['evaluation_device'])
        actor.load_state_dict(start['evaluation_policy'])
        optimizer = torch.optim.Adam(model.parameters(), lr=config['learning_rate'])
        optimizer.load_state_dict(start['optimizer'])
        collector = Collector(training_runtimes, training, 16, spec['seed'], True)
        controller = CaseCurriculum(collector.starts, start['metadata']['controller'])
        if len(start['stream_rngs']) != 16 or len(start['environments']) != 16:
            raise ValueError('checkpoint_replay.streams: incomplete sampler or native state')
        for generator, state in zip(collector.generators, start['stream_rngs']):
            generator.set_state(state)
        for env, state in zip(collector.envs, start['environments']):
            env.restore_state_json(json.dumps(state))
        rng = np.random.default_rng()
        rng.bit_generator.state = start['numpy_rng']
        np.random.set_state(start['numpy_legacy_rng'])
        random.setstate(start['python_rng'])
        torch.set_rng_state(start['torch_rng'])
        torch.cuda.set_rng_state_all(start['cuda_rng'])
        active = 'anchor_assessment'
        assess(0)
        for update_index in range(1, spec['updates']+1):
            active = f'collection-{update_index}'
            levels_now = {key: value['level'] for key, value in controller.state.items()}
            rollout = {key: [] for key in FIELDS}
            for _ in range(4):
                collector.partial, collector.pending, collector.completed_steps = None, None, 0
                levels, reasons = controller.schedule(collector.keys, completed_episodes, original_protocol['rehearsal_every'])
                part, collected_rows = collector.collect(model, levels)
                for key in FIELDS:
                    rollout[key].extend(part[key])
                for case, env, collected, level, reason in zip(collector.keys, collector.envs, collected_rows, levels, reasons):
                    commands += 16
                    completed_episodes += 1
                    prefix = collector.starts[case][level]
                    record = episode_record(case, env, collected, completed_episodes, commands, level, reason,
                                            levels_now[case], prefix, 16)
                    index = (commands-spec['start_commands'])//16-1
                    differences = compare_record(record, history_episodes[index])
                    result['matched_episodes'] += not differences
                    if differences and result['first_episode_mismatch'] is None:
                        result['first_episode_mismatch'] = {'update': update_index, 'commands': commands, 'differences': differences}
                    append(output/'collected.jsonl', {'summary': record, 'prefix': prefix, 'trace': trace(env),
                        'layout': layout(env, 'best'), 'samples': {key: value for key, value in collected.items() if key != 'observations'}})
            active = f'update-{update_index}'
            metrics = measured_update(model, optimizer, rollout, training, rng)
            refresh_actor(model, actor)
            expected = history_updates[update_index-1]
            metric_fields = ('policy_loss', 'value_loss', 'entropy', 'approx_kl', 'gradient_norm', 'optimizer_steps')
            differences = compare_record({key: metrics[key] for key in metric_fields}, {key: expected[key] for key in metric_fields})
            if differences and result['first_metric_mismatch'] is None:
                result['first_metric_mismatch'] = {'update': update_index, 'commands': commands, 'differences': differences}
            state_path = output/f'update-{update_index}.pt'
            save_state(state_path, True)
            result['updates'].append({'update': update_index, 'commands': commands, 'metrics': metrics,
                'historical_metrics_equal': not differences, 'policy_sha256': policy_hash(model.state_dict()),
                'checkpoint': state_path.name, 'checkpoint_sha256': digest(state_path)})
            if update_index % spec['assessment_stride'] == 0:
                active = f'assessment-{update_index}'
                final_gate = assess(update_index)
            write(output/'replay.json', result)
        controller.observe(final_gate['successes'], original_protocol)
        result['endpoint_equal'] = {
            'policy': equal(cpu_state(model.state_dict()), end['policy']), 'evaluation_policy': equal(cpu_state(actor.state_dict()), end['evaluation_policy']),
            'optimizer': equal(cpu_state(optimizer.state_dict()), end['optimizer']),
            'torch_rng': equal(torch.get_rng_state(), end['torch_rng']),
            'cuda_rng': equal(torch.cuda.get_rng_state_all(), end['cuda_rng']),
            'numpy_rng': equal(rng.bit_generator.state, end['numpy_rng']),
            'numpy_legacy_rng': equal(np.random.get_state(), end['numpy_legacy_rng']), 'python_rng': equal(random.getstate(), end['python_rng']),
            'stream_rngs': equal([g.get_state() for g in collector.generators], end['stream_rngs']),
            'controller': equal(controller.state, end['metadata']['controller']),
            'environments': equal([snapshot(env) for env in collector.envs], end['environments'])}
        result['exact_replay'] = (all(result['endpoint_equal'].values()) and result['matched_episodes'] == 2048
                                  and result['assessments'][-1]['historical_fields_equal']['rows'])
        save_state(output/'endpoint.pt', True)
        active = 'independent_replay'
        catalogs = {key: Catalog(root, runtime.path) for key, runtime in runtimes.items()}
        training_replays, setup_commands = 0, 0
        for record in rows(output/'collected.jsonl'):
            summary = record['summary']
            replay(catalogs[summary['case']], training, record['prefix'], record['trace'], record['layout'])
            training_replays += 1
            setup_commands += len(record['prefix'])
        assessment_replays = 0
        for assessment in result['assessments']:
            gate = read(output/assessment['file'])
            for row in gate['rows']:
                actual = replay_assessment(catalogs[row['case']], config, collector.starts[row['case']][row['level']], row, 16)
                if not equal(actual, row):
                    raise ValueError('checkpoint_replay.independent: assessment replay differs')
                assessment_replays += 1
        if training_replays != 2048 or assessment_replays != 9216:
            raise ValueError('checkpoint_replay.accounting: incomplete independent replay matrix')
        if [digest(path) for path in checkpoint_paths] != checkpoint_hashes:
            raise ValueError('checkpoint_replay.final: original checkpoint changed')
        result.update(status='complete', total_seconds=time.perf_counter()-started,
            validation={'training_episodes': training_replays, 'assessment_episodes': assessment_replays,
                        'training_commands': 32768, 'assessment_commands': 147456, 'training_setup_commands': setup_commands},
            collected_sha256=digest(output/'collected.jsonl'), endpoint_sha256=digest(output/'endpoint.pt'))
    except BaseException as error:
        result.update(status='interrupted', diagnostic={'code': 'checkpoint_replay.interrupted', 'severity': 'error',
            'stage': 'checkpoint_update_replay', 'entity': active, 'message': str(error)})
        if all(value is not None for value in (model, actor, optimizer, collector, controller, rng)):
            save_state(output/'interrupted-state.pt', False)
        write(output/'replay.json', result)
        raise
    finally:
        if collector is not None:
            collector.close()
    write(output/'replay.json', result)
    return result


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument('--source', type=Path, required=True)
    parser.add_argument('--protocol', type=Path, required=True)
    parser.add_argument('--native-library', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    torch.set_num_interop_threads(1)
    run(args.root.resolve(), args.source.resolve(), args.protocol.resolve(), args.native_library.resolve(), args.output.resolve())
