"""Quantify a frozen endpoint tradeoff without additional gradient updates."""

import argparse
import json
from pathlib import Path
import subprocess
import time

import numpy as np
import torch

from clock_intervention import paired_counts
from evidence import provenance, write
from mastery_policy_selection import digest, read
from native import Runtime
from native_training_comparison import equal, policy_hash
from policy import Policy
from reference.acceptance import Catalog
from update_size_comparison import quality
from update_stability import evaluate


def bootstrap(differences, seed, draws):
    data = np.asarray(differences, dtype=np.float64)
    if data.ndim != 2 or min(data.shape) == 0 or not np.isfinite(data).all() or draws < 1:
        raise ValueError('endpoint_precision.bootstrap: finite seed-by-case observations required')
    rng = np.random.default_rng(seed)
    case_means = []
    for start in range(0, draws, 100):
        indices = rng.integers(0, len(data), size=(min(100, draws-start), len(data)))
        case_means.append(data[indices].mean(axis=1))
    sampled = np.concatenate(case_means)
    return {'case_intervals': np.quantile(sampled, [0.025, 0.975], axis=0).T.tolist(),
            'mean_interval': np.quantile(sampled.mean(axis=1), [0.025, 0.975]).tolist(),
            'mean_difference': float(data.mean()), 'seed_clusters': len(data), 'draws': draws, 'seed': seed}


def summarize(high, low, seed, draws):
    if not high or [c['case'] for c in high] != [c['case'] for c in low]:
        raise ValueError('endpoint_precision.pairs: identical nonempty case lists required')
    records, differences = [], []
    common_seeds = [r['seed'] for r in high[0]['episodes']]
    for a, b in zip(high, low):
        if (a['initial_score'] != b['initial_score']
                or [r['seed'] for r in a['episodes']] != common_seeds
                or [r['seed'] for r in b['episodes']] != common_seeds):
            raise ValueError('endpoint_precision.pairs: seed or root identity differs')
        def success(row):
            return row['score'][0] < a['initial_score'][0] and bool(row['translated_facilities'])
        delta = [int(success(y))-int(success(x)) for x, y in zip(a['episodes'], b['episodes'])]
        differences.append(delta)
        records.append({'case': a['case'], 'high_successes': sum(success(r) for r in a['episodes']),
            'low_successes': sum(success(r) for r in b['episodes']), 'success_rate_difference': float(np.mean(delta)),
            'low_vs_high': paired_counts(b['episodes'], a['episodes']),
            'mean_normalized_area_difference': float(np.mean([(y['score'][0]-x['score'][0])/a['initial_score'][0]
                                                              for x, y in zip(a['episodes'], b['episodes'])]))})
    intervals = bootstrap(np.asarray(differences).T, seed, draws)
    for row, interval in zip(records, intervals['case_intervals']):
        row['descriptive_95_percent_interval'] = interval
    return {'cases': records, 'bootstrap': intervals,
            'pooled_low_vs_high': {k: sum(r['low_vs_high'][k] for r in records) for k in ('wins', 'ties', 'losses')},
            'quality_exact': {'high': [str(x) for x in quality(high)], 'low': [str(x) for x in quality(low)]}}


def run(root, source, protocol, library, output):
    spec = read(protocol)
    if (spec.get('schema_version') != 1 or spec.get('purpose') != 'frozen_endpoint_precision'
            or spec['continuation_seed'] != 1030000 or spec['multipliers'] != [1, 0.1] or spec['update'] != 32
            or spec['episodes'] != 256 or spec['horizon'] != 128 or spec['batch_size'] != 16
            or spec['sampling_seed_start'] != 1080000 or spec['bootstrap_seed'] != 1090000
            or spec['bootstrap_resamples'] != 10000 or spec['device'] != 'cuda'):
        raise ValueError('endpoint_precision.protocol: unsupported precision diagnostic')
    if output.exists() or subprocess.check_output(['git', 'status', '--porcelain'], cwd=root, text=True).strip():
        raise ValueError('endpoint_precision.source: clean checkout and fresh output required')
    comparison_path = source/'comparison.json'
    if digest(comparison_path) != spec['comparison_sha256']:
        raise ValueError('endpoint_precision.comparison: source comparison hash differs')
    comparison = read(comparison_path)
    if comparison['status'] != 'complete' or comparison['decision']['candidate_for_larger_test'] is not False:
        raise ValueError('endpoint_precision.comparison: expected completed failed criterion')
    if not torch.cuda.is_available():
        raise ValueError('endpoint_precision.cuda: original CUDA runtime required')
    cases = read(root/spec['study_protocol'])['cases']
    if len(cases) != 16 or set(cases) != set(comparison['provenance']):
        raise ValueError('endpoint_precision.cases: expected unchanged sixteen training roots')
    output.mkdir(parents=True)
    result = {'schema_version': 1, 'status': 'running', 'protocol': spec, 'protocol_sha256': digest(protocol),
              'original_criterion_passed': False, 'actors': {}, 'cells': []}
    active, started = 'freeze', time.perf_counter()
    try:
        actors, configs, checkpoints = {}, {}, {}
        for multiplier, name in ((1, 'high'), (0.1, 'low')):
            row = next(r for r in comparison['branches'] if r['seed'] == spec['continuation_seed'] and r['multiplier'] == multiplier)
            directory = source/f"seed{spec['continuation_seed']}-lr{multiplier:g}"
            entry = next(r for r in row['updates'] if r['update'] == 32)
            path = directory/entry['checkpoint']
            if digest(path) != entry['checkpoint_sha256']:
                raise ValueError('endpoint_precision.checkpoint: endpoint hash differs')
            saved = torch.load(path, map_location='cpu', weights_only=False)
            if not equal(saved['policy'], saved['evaluation_policy']) or policy_hash(saved['policy']) != row['final_policy_sha256']:
                raise ValueError('endpoint_precision.actor: frozen actor identity differs')
            config = {**saved['config'], 'horizon': 128}
            configs[name] = config
            torch.set_num_threads(config['threads'])
            torch.backends.cuda.matmul.allow_tf32 = False
            torch.backends.cudnn.allow_tf32 = False
            torch.backends.cudnn.benchmark = False
            runtime = Runtime(root, root/next(iter(cases.values())), config, library)
            actor = Policy(runtime.new_engine().observe()[0], normalization=config['normalization'], width_multiplier=1).to('cuda').eval()
            actor.load_state_dict(saved['policy'])
            actors[name], checkpoints[name] = actor, path
            export = output/f'{name}.pt'
            torch.save({'purpose': 'frozen_precision_diagnostic', 'policy': actor.state_dict(), 'config': config}, export)
            result['actors'][name] = {'checkpoint_sha256': digest(path), 'policy_sha256': policy_hash(actor.state_dict()),
                                     'export_sha256': digest(export)}
        for active, case_path in cases.items():
            runtime = Runtime(root, root/case_path, configs['high'], library)
            current, previous = provenance(runtime, protocol), comparison['provenance'][active]
            if (current['native'] != {**previous['native'], 'library': current['native']['library']}
                    or any(current[k] != previous[k] for k in ('python', 'torch', 'numpy', 'platform', 'machine'))
                    or any(digest(root/'tools/factory-edit-pilot'/name) != previous['files_sha256'][f'tools/factory-edit-pilot/{name}']
                           for name in ('policy.py', 'evaluation_batch.py'))):
                raise ValueError(f'endpoint_precision.provenance: runtime or input changed: {active}')
            if 'provenance' not in result:
                result['provenance'] = current
            catalog = Catalog(root, root/case_path)
            for name, actor in actors.items():
                result['cells'].append(evaluate(actor, runtime, catalog, configs[name], spec, output, active, name))
                write(output/'precision.json', result)
            print(json.dumps({'case': active, 'high_successes': result['cells'][-2]['translated_successes'],
                              'low_successes': result['cells'][-1]['translated_successes']}), flush=True)
        result['summary'] = summarize([c for c in result['cells'] if c['condition'] == 'high'],
                                      [c for c in result['cells'] if c['condition'] == 'low'],
                                      spec['bootstrap_seed'], spec['bootstrap_resamples'])
        if any(policy_hash(actor.state_dict()) != result['actors'][name]['policy_sha256']
               or digest(checkpoints[name]) != result['actors'][name]['checkpoint_sha256'] for name, actor in actors.items()):
            raise ValueError('endpoint_precision.final: frozen input or actor changed')
        if digest(comparison_path) != spec['comparison_sha256'] or len(result['cells']) != 32:
            raise ValueError('endpoint_precision.final: comparison changed or incomplete cells')
        result.update(status='complete', total_seconds=time.perf_counter()-started,
                      validation={'accepted_episodes': 8192, 'selected_trace_replays': 64, 'commands': 1048576, 'updates': 0})
    except BaseException as error:
        result.update(status='interrupted', diagnostic={'code': 'endpoint_precision.interrupted', 'severity': 'error',
            'stage': 'frozen_endpoint_precision', 'entity': active, 'message': str(error)})
        write(output/'precision.json', result)
        raise
    write(output/'precision.json', result)
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
