"""Short, deliberately coarse adaptation screens across data, horizon and supervision."""

import argparse
from copy import deepcopy
import json
from pathlib import Path
import time

import numpy as np
import torch
import torch.nn.functional as functional

from curriculum import Collector
from evidence import append, provenance, write
from geometry_policy_evaluation import admitted_cases
from geometry_restart_diagnosis import evaluate_cell
from mastery_policy_selection import digest, read
from native import Runtime
from native_training_comparison import policy_hash
from policy import Policy, batch, update
from run import refresh_actor


def imitation(model, runtimes, config, optimizer, rng, epochs, directory):
    observations, actions = [], []
    for runtime in runtimes.values():
        engine = runtime.new_engine()
        for command in runtime.case['witness']:
            observation, handle = engine.observe()
            action = json.loads(engine.export_candidates_json(handle)).index(command)
            observations.append(observation)
            actions.append(action)
            _, _, outcome = engine.step(handle, action)
            if not outcome['applied']:
                raise ValueError(f'landscape.teacher: {runtime.path}: invalid witness step')
    device = next(model.parameters()).device
    started = time.perf_counter()
    for epoch in range(epochs):
        losses = []
        permutation = rng.permutation(len(actions))
        for start in range(0, len(actions), config['minibatch']):
            indices = permutation[start:start+config['minibatch']]
            data = {k: v.to(device) for k, v in batch([observations[i] for i in indices]).items()}
            logits, _ = model(data)
            loss = functional.cross_entropy(logits, torch.tensor([actions[i] for i in indices], device=device))
            optimizer.zero_grad(set_to_none=True)
            loss.backward()
            torch.nn.utils.clip_grad_norm_(model.parameters(), config['gradient_norm'], error_if_nonfinite=True)
            optimizer.step()
            losses.append(loss.item())
        model.calibrate_normalization(observations, config['minibatch'])
        append(directory/'imitation.jsonl', {'epoch': epoch+1, 'loss': float(np.mean(losses))})
    return {'unique_teacher_steps': len(actions), 'label_exposures': epochs*len(actions),
            'seconds': time.perf_counter()-started}


def adapt(root, initial, paths, config, arm, spec, library, directory):
    directory.mkdir()
    model, actor = deepcopy(initial), deepcopy(initial)
    optimizer = torch.optim.Adam(model.parameters(), lr=config['learning_rate'])
    rng = np.random.default_rng(spec['seed'])
    config = {**config, 'horizon': arm['horizon']}
    runtimes = {k: Runtime(root, p, config, library) for k, p in paths.items()}
    result = {'arm': arm, 'status': 'running', 'training_cases': list(paths), 'commands': 0, 'updates': 0,
              'setup_commands': 0, 'positive_episodes': 0, 'episodes': 0}
    collector = None
    started = time.perf_counter()
    try:
        if arm['method'].startswith('imitation'):
            teachers = {k: Runtime(root, p, {**config, 'horizon': 16}, library) for k, p in paths.items()}
            result['imitation'] = imitation(model, teachers, config, optimizer, rng, spec['imitation_epochs'], directory)
        if arm['method'] != 'imitation':
            collector = Collector(runtimes, config, len(runtimes), spec['seed'], True, threaded=False)
            round_commands = len(runtimes)*config['horizon']
            while result['commands']+round_commands <= spec['ppo_command_budget']:
                refresh_actor(model, actor)
                levels = [0 if arm['starts'] == 'roots' or rng.random() < .5 else
                          int(rng.integers(1, max(collector.starts[key])+1)) for key in collector.keys]
                rollout, episodes = collector.collect(actor, levels)
                metrics = update(model, optimizer, rollout, config, rng)
                result['commands'] += len(rollout['actions'])
                result['updates'] += 1
                result['episodes'] += len(episodes)
                result['positive_episodes'] += sum(sum(e['rewards']) > 0 for e in episodes)
                result['setup_commands'] += sum(len(collector.starts[k][level]) for k, level in zip(collector.keys, levels))
                append(directory/'updates.jsonl', {'commands': result['commands'], 'levels': levels,
                       'episode_returns': [sum(e['rewards']) for e in episodes], 'metrics': metrics})
        result['status'] = 'complete'
    except Exception as error:
        result.update(status='error', error=str(error))
    finally:
        saved = {'policy': model.state_dict(), 'optimizer': optimizer.state_dict(), 'config': config,
                 'arm': arm, 'result': result, 'numpy_rng': rng.bit_generator.state,
                 'torch_rng': torch.get_rng_state(), 'collector_rng': [] if collector is None else
                 [g.get_state() for g in collector.generators]}
        torch.save(saved, directory/'final.pt')
        if collector is not None:
            collector.close()
    result.update(seconds=time.perf_counter()-started, checkpoint_sha256=digest(directory/'final.pt'),
                  policy_sha256=policy_hash(model.state_dict()))
    write(directory/'training.json', result)
    return model.eval(), result


def run(root, protocol_path, library, output):
    spec = read(protocol_path)
    if output.exists():
        raise ValueError(f'landscape.output: {output}: choose a fresh directory')
    checkpoint = root/spec['anchor']
    if digest(checkpoint) != spec['anchor_sha256']:
        raise ValueError(f'landscape.anchor: {checkpoint}: changed starting checkpoint')
    config = read(root/spec['base_config'])
    torch.set_num_threads(config['threads'])
    torch.manual_seed(spec['seed'])
    groups = admitted_cases(root, read(root/spec['geometry_protocol']))
    broad = groups['train'] | groups['development']
    all_cases = broad | groups['heldout']
    runtime = Runtime(root, next(iter(groups['train'].values())), config, library)
    source = provenance(runtime, protocol_path)
    if not source['git_worktree_clean']:
        raise ValueError('landscape.source: commit exploratory source before execution')
    saved = torch.load(checkpoint, map_location='cpu', weights_only=False)
    initial = Policy(runtime.new_engine().observe()[0], normalization=config['normalization'], width_multiplier=1).to(config['device']).eval()
    initial.load_state_dict(saved['evaluation_policy'])
    output.mkdir(parents=True)
    result = {'schema_version': 1, 'status': 'running', 'evidence_level': 'single_seed_exploratory_screen',
              'protocol': spec, 'provenance': source, 'opened_geometries': list(all_cases),
              'training_excluded_geometries': list(groups['heldout']), 'branches': []}
    started = time.perf_counter()

    def evaluate(model, name):
        cells = []
        before = policy_hash(model.state_dict())
        for key, path in all_cases.items():
            if time.perf_counter()-started >= spec['maximum_seconds']:
                raise TimeoutError('landscape.budget: whole-screen elapsed guard exhausted')
            runtime = Runtime(root, path, {**config, 'horizon': spec['evaluation_horizon']}, library)
            cell = evaluate_cell(root, runtime, model, config,
                {'episodes': spec['evaluation_episodes'], 'total_commands': spec['evaluation_commands'], 'required_successes': 1},
                spec['evaluation_horizon'], spec['evaluation_seed_start'], output/name/'evaluation'/key)
            cell.pop('passed')
            cell.update(case=key, initial_score=runtime.new_engine().initial_score,
                        split=next(split for split, paths in groups.items() if key in paths))
            cells.append(cell)
        if policy_hash(model.state_dict()) != before:
            raise ValueError('landscape.mutation: evaluation changed model tensors')
        return cells

    try:
        result['baseline'] = evaluate(initial, 'baseline')
        write(output/'screen.json', result)
        for arm in spec['arms']:
            if time.perf_counter()-started >= spec['maximum_seconds']:
                raise TimeoutError('landscape.budget: whole-screen elapsed guard exhausted')
            torch.manual_seed(spec['seed'])
            model, record = adapt(root, initial, groups['train'] if arm['split'] == 'old' else broad,
                                  config, arm, spec, library, output/arm['id'])
            result['branches'].append(record)
            if record['status'] == 'complete':
                try:
                    record['evaluation'] = evaluate(model, arm['id'])
                except Exception as error:
                    record.update(status='evaluation_error', error=str(error))
            write(output/'screen.json', result)
            print(json.dumps({'arm': arm['id'], 'status': record['status'], 'commands': record['commands'],
                  'successes': sum(c['translated_successes'] for c in record.get('evaluation', []))}), flush=True)
        result['status'] = 'complete'
    except Exception as error:
        result.update(status='interrupted', error=str(error))
    result['seconds'] = time.perf_counter()-started
    result['anchor_unchanged'] = digest(checkpoint) == spec['anchor_sha256']
    write(output/'screen.json', result)
    return result


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--protocol', type=Path, required=True)
    parser.add_argument('--native-library', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    torch.set_num_interop_threads(1)
    run(Path(__file__).resolve().parents[2], args.protocol.resolve(), args.native_library.resolve(), args.output.resolve())
