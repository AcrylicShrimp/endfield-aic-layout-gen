"""Measure thread and batch costs on frozen schema-2 actor observations."""

import argparse
from copy import deepcopy
import json
from pathlib import Path
import statistics
import subprocess

import torch

from actor_benchmark import measure
from evidence import provenance, write
from mastery_policy_selection import digest
from native import Runtime, layout, trace
from native_training_comparison import policy_hash
from policy import Policy, batch, single
from reference.acceptance import Catalog, check


@torch.no_grad()
def capture(root, runtime, actor, episodes, horizon, seed_start):
    observations, traces = [], []
    for index in range(episodes):
        env = runtime.new_engine()
        generator = torch.Generator().manual_seed(seed_start+index)
        for _ in range(horizon):
            observation, handle = env.observe()
            action, _, _ = actor.act(observation, generator)
            observations.append(observation)
            env.step(handle, action)
        retained = layout(env, 'best')
        if not check(Catalog(root, runtime.path), retained)['valid']:
            raise ValueError('entity_actor.acceptance: captured episode retained an invalid layout')
        traces.append({'seed': seed_start+index, 'commands': trace(env), 'best': retained})
    return observations, traces


@torch.no_grad()
def verify(actor, observations, device, size, expected):
    length = len(observations)//size
    maximum = 0.0
    for start in range(length):
        indices = [start+i*length for i in range(size)]
        data = single(observations[indices[0]]) if size == 1 else batch([observations[i] for i in indices])
        actual = actor({key: value.to(device) for key, value in data.items()})
        for row, index in enumerate(indices):
            logits = actual[0][row, :len(observations[index]['candidates'])].cpu()
            value = actual[1][row].cpu()
            for result, reference in ((logits, expected[index][0]), (value, expected[index][1])):
                torch.testing.assert_close(result, reference, atol=1e-5, rtol=1e-4)
                maximum = max(maximum, float((result-reference).abs().max()))
    return maximum


def run(root, spec_path, library, output):
    spec = json.loads(spec_path.read_text())
    if (spec.get('schema_version') != 1 or output.exists() or spec['repeats'] < 1
            or spec['episodes'] < 1 or spec['horizon'] < 1
            or any(t < 1 for t in spec['cpu_threads'])
            or any(b < 1 or spec['episodes'] % b for b in spec['batch_sizes'])):
        raise ValueError('entity_actor.protocol: invalid or nonfresh benchmark')
    if subprocess.check_output(['git', 'status', '--porcelain'], cwd=root, text=True).strip():
        raise ValueError('entity_actor.source: commit benchmark before measurement')
    output.mkdir(parents=True)
    result = {'schema_version': 1, 'status': 'running', 'protocol': spec, 'protocol_sha256': digest(spec_path), 'cells': []}
    try:
        for width, source_path in spec['sources'].items():
            source = root/source_path
            metadata = json.loads((source/'metadata.json').read_text())
            audit = json.loads((source/'study-audit.json').read_text())
            checkpoint = source/f'checkpoint-{spec["commands"]}.pt'
            if (metadata['status'] != 'complete' or audit['status'] != 'passed'
                    or audit['source_commit'] != spec['source_commit']
                    or digest(source/'metadata.json') != audit['metadata_sha256']
                    or digest(checkpoint) != audit['checkpoints_sha256'][str(spec['commands'])]
                    or metadata['config']['width_multiplier'] != int(width)):
                raise ValueError(f'entity_actor.source: unaudited or mismatched {source}')
            saved = torch.load(checkpoint, map_location='cpu', weights_only=False)
            config = {**metadata['config'], 'device': 'cpu', 'horizon': spec['horizon']}
            for key in spec['cases']:
                path = root/metadata['protocol']['cases'][key]
                runtime = Runtime(root, path, config, library)
                original = metadata['provenance'][key]
                current = provenance(runtime, spec_path)
                required = [path, root/'tools/factory-edit-pilot/policy.py', *runtime.paths.values()]
                if (current['native']['build']['files_sha256'] != original['native']['build']['files_sha256']
                        or any(digest(p) != original['files_sha256'][str(p.relative_to(root))] for p in required)):
                    raise ValueError(f'entity_actor.source: changed native/policy/case/catalog {key}')
                torch.set_num_threads(16)
                observation, _ = runtime.new_engine().observe()
                actor = Policy(observation, normalization=config['normalization'], width_multiplier=int(width))
                actor.load_state_dict(saved['evaluation_policy'])
                actor.eval()
                before = policy_hash(actor.state_dict())
                observations, traces = capture(root, runtime, actor, spec['episodes'], spec['horizon'], spec['seed_start'])
                with torch.no_grad():
                    expected = [(value[0][0].clone(), value[1][0].clone()) for value in (actor(single(o)) for o in observations)]
                destination = output/f'width{width}'/key
                destination.mkdir(parents=True)
                torch.save({'schema_version': 2, 'config': config, 'policy': actor.state_dict(), 'observations': observations}, destination/'input.pt')
                write(destination/'capture.json', traces)
                rows, summaries = [], []
                variants = [('cpu', threads, size) for threads in spec['cpu_threads'] for size in spec['batch_sizes']]
                variants.append(('cuda', 16, max(spec['batch_sizes'])))
                gpu = deepcopy(actor).to('cuda')
                for device, threads, size in variants:
                    model = actor if device == 'cpu' else gpu
                    torch.set_num_threads(threads)
                    error = verify(model, observations, device, size, expected)
                    summaries.append({'device': device, 'threads': threads, 'batch_size': size, 'maximum_absolute_error': error})
                for repeat in range(spec['repeats']+1):
                    for device, threads, size in variants[::1 if repeat % 2 == 0 else -1]:
                        torch.set_num_threads(threads)
                        timing = measure(actor if device == 'cpu' else gpu, observations, size, device, False)
                        if not timing['finite']:
                            raise ValueError('entity_actor.nonfinite: actor output is nonfinite')
                        rows.append({'device': device, 'threads': threads, 'batch_size': size, 'repeat': repeat, 'warmup': repeat == 0, **timing})
                for summary in summaries:
                    selected = [r for r in rows if not r['warmup'] and all(r[k] == summary[k] for k in ('device', 'threads', 'batch_size'))]
                    summary['seconds'] = statistics.median(r['seconds'] for r in selected)
                if policy_hash(actor.state_dict()) != before or policy_hash(gpu.cpu().state_dict()) != before:
                    raise ValueError('entity_actor.mutation: benchmark changed policy or BN buffers')
                cell = {'width': int(width), 'case': key, 'checkpoint_sha256': digest(checkpoint),
                        'policy_sha256': before, 'provenance': current, 'input_sha256': digest(destination/'input.pt'),
                        'observations': len(observations), 'rows': rows, 'summary': summaries}
                write(destination/'result.json', cell)
                result['cells'].append({k: v for k, v in cell.items() if k not in ('rows', 'provenance')})
                write(output/'result.json', result)
                print(json.dumps(result['cells'][-1]), flush=True)
        result['status'] = 'passed'
    except BaseException as error:
        result.update(status='interrupted', diagnostic={'code': 'entity_actor.interrupted', 'severity': 'error',
            'stage': 'entity_actor', 'message': str(error), 'entity': str(output)})
        raise
    finally:
        write(output/'result.json', result)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--spec', type=Path, required=True)
    parser.add_argument('--native-library', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    torch.set_num_interop_threads(1)
    torch.backends.cuda.matmul.allow_tf32 = False
    torch.backends.cudnn.allow_tf32 = False
    torch.backends.cudnn.benchmark = False
    run(Path(__file__).resolve().parents[2], args.spec.resolve(), args.native_library.resolve(), args.output.resolve())


if __name__ == '__main__':
    main()
