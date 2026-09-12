"""One-off retained-evidence analysis: six final checkpoints and 192 final traces.

No training, native execution, optimizer replay, or model inference is performed.
Run from the frozen experiment root; the report schema describes observations,
not an infeasibility proof or a feasible construction from the geometric bound.
"""
import argparse
from collections import Counter, defaultdict
import json
from pathlib import Path
from statistics import mean, median
import sys

sys.path.insert(0, str(Path.cwd() / 'tools/factory-edit-pilot'))
from blueprint_ppo_store import checked, load, read, reference, sha, write

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--matrix', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
args = parser.parse_args()
assert not args.output.exists(), args.output
matrix = args.matrix.resolve()
protocol = read(matrix / 'matrix.json')
assert read(matrix / 'state.json')['status'] == 'complete'
inputs = {r['identity']['slot']: r for p in sorted((matrix / 'training-inputs').glob('episode-*/episode.json'))
          for r in [read(p)]}
geometry = {}
arms = []

def floor(observation):
    bp = observation['blueprint']
    points = [n['initial_position'] for n in bp['nodes']
              if not (n['terminal'] and n['terminal']['endpoint']['kind'] == 'facility')]
    points += [c['position'] for c in bp['components']]
    bound = ((max(p['x'] for p in points)-min(p['x'] for p in points)+1) *
             (max(p['y'] for p in points)-min(p['y'] for p in points)+1)) if points else 0
    original = observation['initial_metrics']['score'][0]
    return dict(facilities=len(observation['facilities']), edges=len(bp['edges']), fixed_points=len(points),
                initial_area=original, fixed_area_floor=bound, area_reduction_ceiling_percent=100*(original-bound)/original)

def average(rows, field):
    return mean(row[field] for row in rows) if rows else None

for arm in protocol['arms']:
    directory = matrix / arm['directory']
    job = read(directory / 'state.json')['jobs'][str(arm['seed'])]
    reports = [read(directory / ref['path']) for ref in job['evaluations']]
    records = [read(directory / ref['path']) for ref in job['records']]
    details = []
    for row, ref in zip(reports[-1]['rows'], reports[-1]['cases'], strict=True):
        case = load(checked(ref, directory))
        identity = str(row['generation_seed'])
        info = floor(case['initial'])
        assert geometry.get(identity, info) == info
        geometry[identity] = info
        counters = Counter()
        diagnostics = Counter()
        positive_at = []
        for entry in case['trace']:
            action, outcome, observation = entry['action'], entry['outcome'], entry['observation']
            kind, status = action['kind'], outcome['status']
            counters['actions'] += 1
            counters[f'{kind}/attempts'] += 1
            counters[f'{kind}/{status}'] += 1
            for diagnostic in outcome['diagnostics']:
                diagnostics[f'{kind}/{diagnostic["code"]}'] += 1
            counters['actions_with_pending_after'] += entry['pending'] > 0
            counters['actions_with_conflicts_after'] += entry['conflicts'] > 0
            if kind == 'pose':
                f = next(f for f in observation['facilities'] if f['instance'] == action['instance'])
                counters['pose/rotation_changes'] += action['rotation'] != f['rotation']
                distance = abs(action['x']-f['x'])+abs(action['y']-f['y'])
                counters['pose/local_distance_le2'] += distance <= 2
                counters['pose/detached_edges'] += len(outcome['details'].get('detached_edges', []))
                counters['pose/conflicts_after'] += entry['conflicts'] > 0
            if kind in ('repair','repair_all'):
                before_pending = sum(e['status']=='pending' for e in observation['edges'])
                counters[f'{kind}/pending_reduced'] += entry['pending'] < before_pending
                counters[f'{kind}/no_pending_before'] += before_pending == 0
            if kind == 'commit' and status in ('validated','improved'):
                counters['commit/valid'] += 1
            if outcome['reward'] > 0:
                positive_at.append(entry['policy_action'])
                counters['positive_area_commits'] += 1
        details.append(dict(id=row['generation_seed'], trace=ref,
            area_reduction_percent=row['total_area_reduction_percent'], counters=dict(counters),
            diagnostics=dict(diagnostics), positive_at=positive_at,
            final_pending=case['trace'][-1]['pending'], final_conflicts=case['trace'][-1]['conflicts'],
            unfinished_draft=case['result']['unfinished_draft']))
    saved = load(checked(job['latest_checkpoint'], directory))['buffer']
    replay = defaultdict(lambda: dict(entries=0, positive_commits=0, unique_episodes=set(), labels=Counter(), uses=0))
    lengths, last_rewards = [], []
    for entry in saved['entries'].values():
        segment = entry['segment']
        slot = segment['episode'] * 8 + segment['stream']
        group = replay[inputs[slot]['draw']['product']]
        group['entries'] += 1
        group['positive_commits'] += entry['occurrences']
        group['unique_episodes'].add(slot)
        group['uses'] += entry['uses']
        group['labels'].update(label['stage'] for label in entry['labels'])
        lengths.append(len(segment['references']))
        last_rewards.extend(label['return_to_terminal'] for label in entry['labels'] if label['stage']=='commit')
    for group in replay.values():
        group['unique_episodes'] = len(group['unique_episodes'])
        group['labels'] = dict(group['labels'])
    assert sum(g['positive_commits'] for g in replay.values()) == saved['positive_commits']
    groups = {}
    for group_name, predicate in [('all',lambda _:True), ('small_2_3',lambda n:n<=3), ('large_16_26',lambda n:n>=16)]:
        selected = [d for d in details if predicate(geometry[str(d['id'])]['facilities'])]
        selected_ids = {d['id'] for d in selected}
        aggregate = Counter()
        rejected = Counter()
        for d in selected:
            aggregate.update(d['counters']); rejected.update(d['diagnostics'])
        blocks = []
        for start in (0,128,256,384):
            points = [r for report in reports if start < report['update'] <= start+128
                      for r in report['rows'] if r['generation_seed'] in selected_ids]
            blocks.append(dict(mean_area_percent=average(points,'total_area_reduction_percent'),
                improved_cases=sum(r['total_area_reduction_percent']>0 for r in points), cases=len(points),
                unfinished_cases=sum(r['unfinished_draft'] for r in points)))
        milestones = {}
        for boundary in (8,32,64):
            values = [100*(r['start_score'][0]-m['best_score'][0])/r['start_score'][0]
                      for report in reports if report['update']>384 for r in report['rows']
                      if r['generation_seed'] in selected_ids for m in r['action_milestones'] if m['policy_action']==boundary]
            milestones[str(boundary)] = mean(values)
        groups[group_name] = dict(cases=len(selected), final_mean=average(selected,'area_reduction_percent'),
            final_improved=sum(d['area_reduction_percent']>0 for d in selected),
            final_valid_commit_cases=sum(d['counters'].get('commit/valid',0)>0 for d in selected),
            final_no_commit_attempt_cases=sum(d['counters'].get('commit/attempts',0)==0 for d in selected),
            final_unfinished=sum(d['unfinished_draft'] for d in selected),
            final_pending_cases=sum(d['final_pending']>0 for d in selected),
            final_conflict_cases=sum(d['final_conflicts']>0 for d in selected),
            final_pending_mean=average(selected,'final_pending'), counters=dict(aggregate), diagnostics=dict(rejected),
            quarter_blocks=blocks, last_window_action_milestones=milestones)
    arms.append(dict(name=arm['name'],variant=arm['variant'],seed=arm['seed'],groups=groups,
        final_case_details=details, success_buffer_by_product=dict(replay),
        successful_segment_lengths=dict(count=len(lengths),median=median(lengths),max=max(lengths)),
        commit_return_mean=mean(last_rewards),
        training=dict(updates=len(records),actor_early_stop_batches=sum(r['ppo']['actor_epochs']<4 for r in records),
            late_actor_early_stop_batches=sum(r['ppo']['actor_epochs']<4 for r in records[-128:]),
            late_mean_actor_epochs=mean(r['ppo']['actor_epochs'] for r in records[-128:]),
            late_mean_last_epoch_kl=mean(r['ppo']['epochs'][-1]['approximate_kl'] for r in records[-128:]))))
    print(arm['name'], 'complete', flush=True)

result=dict(schema='blueprint-retained-bottleneck-analysis-v1', matrix=reference(matrix/'matrix.json',matrix),
    analysis_source_sha256=sha(Path(__file__)), scope=dict(final_trace_files=192,final_checkpoints=6,
    training_rollouts_replayed=0,new_policy_actions=0),
    geometry=geometry, arms=arms, generated_input_products=dict(Counter(r['draw']['product'] for r in inputs.values())))
write(args.output,result)
print(args.output,flush=True)
