"""Compare four fresh spatial actors on an identical frozen factory selection set."""
import argparse
from collections import Counter, defaultdict
import json
import math
from pathlib import Path

from compare_region_pilot import (records, completed, reduction, traces, trace_stats,
                                  action_category, future_credit, read, sha, mean)
from ppo_spatial_model import VARIANTS, specification


def pose_breakdown(episodes):
    buckets, diagnostics = defaultdict(list), Counter()
    for episode in episodes.values():
        for step in episode['trace']:
            if not step.get('acceptance', {}).get('candidate_validated', False):
                diagnostics.update(d['code'] for d in step['diagnostics'])
            action = step['action']
            if action['kind'] != 'pose':
                continue
            members = len(action['poses'])
            labels = ['group' if members > 1 else 'single']
            if members == 1:
                labels.append('single-footprint-' + 'x'.join(map(str, sorted([action['width'], action['height']]))))
            region = step.get('region')
            labels.append('with-route-chains' if region and region['initial_chains'] else 'without-route-chains')
            for label in labels:
                buckets[label].append(step)
    return dict(groups={label: dict(selected=len(rows),
                validated=sum(s.get('acceptance', {}).get('candidate_validated', False) for s in rows),
                best_area_improved=sum(s['reward_numerator'] > 0 for s in rows),
                direct_area_gain_pp=100*sum(s['reward'] for s in rows)/len(episodes))
                for label, rows in buckets.items()}, rejection_diagnostic_occurrences=dict(diagnostics))


def resource_summary(root):
    samples = [json.loads(line) for p in root.glob('sessions/*/resources*.jsonl') for line in p.read_text().splitlines()]
    return dict(samples=len(samples), peak_sampled_host_rss_bytes=max((s['rss_bytes'] for s in samples), default=None),
                peak_sampled_gpu_used_mib=max((g['used_mib'] for s in samples for g in s['gpu']), default=None),
                note='Sampled process-tree RSS and device usage, not exact allocator peaks.')


def compare(root, output, batches):
    runs = {name: root/f'ppo-spatial-{name}' for name in VARIANTS}
    data, states, protocols, identities = {}, {}, {}, {}
    for name, directory in runs.items():
        states[name], data[name] = records(directory, batches)
        protocols[name] = read(directory/'protocol.json')
        assert states[name]['status'] == 'complete'
        assert protocols[name]['spec']['model'] == specification(name)
        assert protocols[name]['config']['training']['batches'] == batches
        assert sorted(b for kind,b in data[name] if kind=='training') == list(range(1,batches+1))
        identities[name] = {(r['policy_sha256'],r['critic_sha256']) for r in data[name]['evaluation',0]['results']}
        assert len(identities[name]) == 1
    assert len({next(iter(v))[1] for v in identities.values()}) == 1, 'critic initialization differs'
    assert identities['cnn4'] == identities['cnn4-dilated'], 'matched four-layer weights differ'
    baseline = data['cnn2']
    phases = set(baseline)
    matched = set()
    controls = ['training','evaluation','execution','ppo','input_manifest','seeds']
    for name, rows in data.items():
        assert set(rows) == phases
        assert all(protocols[name]['config'][k] == protocols['cnn2']['config'][k] for k in controls)
        assert protocols[name]['spec']['selection'] == protocols['cnn2']['spec']['selection']
        for phase, record in rows.items():
            ref = {r['identity']['generation_seed']:r for r in baseline[phase]['results']}
            current = {r['identity']['generation_seed']:r for r in record['results']}
            assert set(ref) == set(current)
            for seed, result in current.items():
                assert result['status'] == ref[seed]['status'], (name, phase, seed, 'episode coverage')
                if result['status'] == 'complete':
                    assert result['row']['budget_complete'] and result['row']['selected'] == protocols[name]['config']['training']['horizon']
                for key in ['status','request_sha256','initial_sha256','geometry_sha256','initial_metrics']:
                    assert result['generation'].get(key) == ref[seed]['generation'].get(key), (name,phase,seed,key)
                matched.add(seed)
    evaluation = sorted(b for kind,b in phases if kind=='evaluation')
    cohort = sorted(set.intersection(*(set(completed(rows['evaluation',b])) for rows in data.values() for b in evaluation)))
    assert len(cohort) == protocols['cnn2']['config']['evaluation']['attempts'], 'evaluation coverage loss'
    report = dict(schema='ppo-spatial-comparison-v1', batches=batches, matched_unique_starts=len(matched),
                  cohort=cohort, shared_initial_critic_sha256=next(iter(identities['cnn2']))[1],
                  four_layer_initial_actor_weights_identical=True, variants={})
    for name, rows in data.items():
        initial = completed(rows['evaluation',0])
        item = dict(model=specification(name), source=dict(root=str(runs[name]),
                    protocol_sha256=sha(runs[name]/'protocol.json'), state_sha256=sha(runs[name]/'state.json')),
                    initial_actor_sha256=next(iter(identities[name]))[0], evaluation=[])
        for batch in evaluation:
            results = completed(rows['evaluation',batch])
            reference = completed(baseline['evaluation',batch])
            episodes = traces(rows['evaluation',batch])
            scores = [reduction(results[s]) for s in cohort]
            differences = [reduction(results[s])-reduction(reference[s]) for s in cohort]
            strata = defaultdict(list)
            for seed in cohort:
                result = results[seed]
                instances = result['row']['instances']
                size = '1-4' if instances<=4 else '5-16' if instances<=16 else '17+'
                strata['instances-'+size].append(seed)
                strata['product-'+result['generation']['draw']['product']].append(seed)
            stats = trace_stats(episodes)
            contributions = {k: 100*v['reward_sum']/len(cohort) for k,v in stats['categories'].items()
                             if k not in ('pose','pose_displacement_ge8')}
            assert math.isclose(sum(contributions.values()),mean(scores),abs_tol=1e-9)
            trajectory = [0.] + [mean([100*(1-e['trace'][t]['best_score'][0]/e['initial_metrics']['score'][0])
                                       for e in episodes.values()]) for t in range(protocols[name]['config']['training']['horizon'])]
            item['evaluation'].append(dict(batch=batch, mean_area_reduction_pp=mean(scores),
                learning_delta_pp=mean([reduction(results[s])-reduction(initial[s]) for s in cohort]),
                minus_cnn2_pp=mean(differences), wins=sum(d>1e-9 for d in differences),
                ties=sum(abs(d)<=1e-9 for d in differences), losses=sum(d< -1e-9 for d in differences),
                pairs=[dict(seed=s, reduction_pp=reduction(results[s]), minus_cnn2_pp=d) for s,d in zip(cohort,differences)],
                strata={label:dict(cases=len(seeds), mean_area_reduction_pp=mean([reduction(results[s]) for s in seeds]),
                         learning_delta_pp=mean([reduction(results[s])-reduction(initial[s]) for s in seeds]),
                         minus_cnn2_pp=mean([reduction(results[s])-reduction(reference[s]) for s in seeds])) for label,seeds in strata.items()},
                trajectory=trajectory, trace_stats=stats, poses=pose_breakdown(episodes),
                action_contributions_pp=contributions, future_credit=future_credit(episodes)))
        training = [rows['training',b] for b in range(1,batches+1)]
        item['training'] = dict(selected=sum(r['quality']['selected'] for r in training),
            episode_worker_seconds=sum(r['quality']['total_episode_seconds'] for r in training),
            generation_worker_seconds=sum(r['quality']['total_generation_seconds'] for r in training),
            update_seconds=sum(r['update_seconds'] for r in training),
            phase_seconds=sum(r['phase_seconds'] for r in training),
            timing={k:sum(r['ppo']['timing'][k] for r in training) for k in training[0]['ppo']['timing']},
            kl_stops=sum(r['ppo']['kl_stopped'] for r in training),
            total_epochs=sum(len(r['ppo']['epochs']) for r in training),
            optimizer_steps=sum(r['ppo']['optimizer_steps'] for r in training),
            batches=[dict(batch=r['phase']['batch'],epochs=len(r['ppo']['epochs']),kl_stopped=r['ppo']['kl_stopped'],
                          final_epoch=r['ppo']['epochs'][-1]) for r in training])
        item['resources'] = resource_summary(runs[name])
        report['variants'][name] = item
    report['planned_contrasts'] = []
    for left, right in [('cnn4', 'cnn4-dilated'), ('cnn4-dilated', 'cnn8')]:
        points = []
        for before, after in zip(report['variants'][left]['evaluation'], report['variants'][right]['evaluation']):
            assert before['batch'] == after['batch']
            delta = [b['reduction_pp']-a['reduction_pp'] for a,b in zip(before['pairs'],after['pairs'])]
            assert all(a['seed']==b['seed'] for a,b in zip(before['pairs'],after['pairs']))
            points.append(dict(batch=before['batch'], right_minus_left_pp=mean(delta),
                               learning_delta_difference_pp=after['learning_delta_pp']-before['learning_delta_pp'],
                               wins=sum(d>1e-9 for d in delta), ties=sum(abs(d)<=1e-9 for d in delta),
                               losses=sum(d< -1e-9 for d in delta), paired_differences_pp=delta))
        report['planned_contrasts'].append(dict(left=left,right=right,evaluation=points))
    report['interpretation'] = dict(quality='Mean per-factory best-area reduction from the identical initial factory.',
        selection='One training seed and reused evaluation factories; selection evidence only.',
        pose_groups='Conditional on selected actions. Routed labels mean a nonempty initial region chain set; categories overlap.',
        footprint='Single-facility pose width/height, sorted to ignore rotation.',
        future_credit='Descriptive reward-to-go, not causal contribution. Direct contributions sum to outcome.',
        critic='Fixed architecture and initial parameters; each critic trains on its own actor trajectories.')
    output.parent.mkdir(parents=True,exist_ok=True)
    output.write_text(json.dumps(report,indent=2,allow_nan=False)+'\n')
    for name,item in report['variants'].items():
        print(name,[(v['batch'],round(v['mean_area_reduction_pp'],4),round(v['learning_delta_pp'],4)) for v in item['evaluation']])


if __name__ == '__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--runs-root',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--batches',type=int,default=32)
    args=parser.parse_args()
    compare(args.runs_root.resolve(),args.output.resolve(),args.batches)
