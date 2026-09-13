"""Analyze bounded saved episodes locally without model execution or new rollouts."""
import argparse
from collections import Counter, defaultdict
from pathlib import Path
import statistics
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from blueprint_ppo_store import checked, load, read, reference, write
from blueprint_ppo_update import require


def average(values):
    return statistics.mean(values) if values else None


def evaluation(path, strata):
    value = read(path)
    groups = {}
    for name, products in strata.items():
        rows = [r for r in value['rows'] if r['product'] in products]
        counts = sum((Counter(r['counts']) for r in rows), Counter())
        actions = sum(r['policy_actions'] for r in rows)
        groups[name] = dict(factories=len({r['generation_seed'] for r in rows}),
            episodes=len(rows), actions=actions, counts=dict(counts),
            action_share={k.removeprefix('kind/'): v / actions for k,v in counts.items() if k.startswith('kind/')},
            pending_share=counts['actions_with_pending']/actions,
            overlap_share=counts['actions_with_overlap']/actions,
            rejection_share=counts['status/rejected']/actions,
            mean_area_reduction_percent=average([r['total_area_reduction_percent'] for r in rows]),
            mean_reduction_by_action={str(step): average([
                100*(r['start_score'][0]-m['best_score'][0])/r['start_score'][0]
                for r in rows for m in r['action_milestones'] if m['policy_action']==step])
                for step in (8,32,64)})
    return dict(update=value['update'],by_stratum=groups)


def describe(sample):
    action = sample['action']
    row = dict(step=sample['policy_action'],kind=action['kind'],status=sample['outcome']['status'],
        reward=sample['reward'],before_best=sample['observation']['best_metrics']['score'],
        after_best=sample['outcome']['best_metrics']['score'],
        pending_after=sample['after_pending'],conflicts_after=sample['after_conflicts'])
    if action['kind']=='place':
        facilities=sample['observation']['facilities']
        index, before = next((i,f) for i,f in enumerate(facilities) if f['instance']==action['instance'])
        row.update(facility_alias=f'F{index}',facility=before['facility'],
            translation=[action['x']-before['x'],action['y']-before['y']],
            rotation_before=before['rotation'],rotation_after=action['rotation'],
            ports_changed=sum(p['port']!=sample['observation']['port_assignments'][p['node']] for p in action['ports']))
    else:
        row.update({key:action[key] for key in ('axis','at','count')})
    return row


def trajectories(root, ordinals, strata, output):
    streams=defaultdict(list); initial={}; completed={}; provenance=[]
    for ordinal in ordinals:
        update_path=root/f'updates/{ordinal:04d}.json'
        ref=read(update_path)['chunk']; chunk=load(checked(ref,root))
        provenance.append(dict(update=reference(update_path,root),chunk=ref))
        for before in chunk['native_before']:
            if before['policy_actions']==0:
                initial[(before['index'],before['episode'])]=before
        for sample in chunk['samples']:
            streams[(sample['stream'],sample['episode'])].append(sample)
        for finished in chunk['completed']:
            completed[(finished['stream'],finished['episode'])]=finished
    episodes=[]
    for key,samples in sorted(streams.items()):
        require(key in initial and key in completed and samples[-1]['terminal'] and
            [s['policy_action'] for s in samples]==list(range(1,len(samples)+1)),
            f'behavior requires complete contiguous episode: {key}')
        before=initial[key]; result=completed[key]['result']; metadata=before['metadata']['training_input']
        product=metadata['draw']['product']
        name=next(name for name,products in strata.items() if product in products)
        segment=[];successes=[]; counts=Counter();diagnostics=Counter();gains=Counter()
        place_attempts=Counter(); rows=[]
        for sample in samples:
            row=describe(sample); rows.append(row)
            counts[f'kind/{row["kind"]}']+=1;counts[f'status/{row["status"]}']+=1
            diagnostics.update(d['code'] for d in sample['outcome']['diagnostics'])
            if row['kind']=='place':
                place_attempts['all']+=1
                place_attempts['translation_changed']+=any(row['translation'])
                place_attempts['rotation_changed']+=row['rotation_before']!=row['rotation_after']
                place_attempts['ports_changed']+=row['ports_changed']>0
                place_attempts['native_state_changed']+=sample['state_changed']
            if sample['state_changed']:segment.append(row)
            if sample['reward']>0:
                gains[row['kind']]+=sample['reward']
                successes.append(dict(final_kind=row['kind'],preparation_actions=len(segment)-1,
                    preparation_kinds=dict(Counter(r['kind'] for r in segment[:-1])),actions=segment))
                segment=[]
        episodes.append(dict(stream=key[0],episode=key[1],stratum=name,product=product,
            input=metadata,facilities=len(before['native']['original']['placements']),
            initial_score=before['metadata']['initial_score'],best_score=result['best_metrics']['score'],
            reduction_percent=100*sum(s['reward'] for s in samples),
            unfinished_draft=result['unfinished_draft'],counts=dict(counts),diagnostics=dict(diagnostics),
            place_attempts=dict(place_attempts),reward_by_final_action=dict(gains),successes=successes,
            actions=rows))
    examples=[]
    for name in strata:
        candidates=[e for e in episodes if e['stratum']==name]
        if not candidates:continue
        chosen=max(candidates,key=lambda e:e['reduction_percent'])
        key=(chosen['stream'],chosen['episode']);prefix=f'{name}-stream-{key[0]}'
        write(output/f'{prefix}-initial.json',initial[key]['native']['original'])
        write(output/f'{prefix}-best.json',completed[key]['result']['best'])
        examples.append(dict(stratum=name,stream=key[0],episode=key[1],
            selection='largest reduction within this fixed contiguous eight-episode training sample',
            initial=f'{prefix}-initial.json',best=f'{prefix}-best.json'))
    return dict(chunks=provenance,episodes=episodes,examples=examples,
        scope='Recorded training actions from the policies used to collect these chunks; evaluation checkpoints are reported separately.',
        attribution='Final-action reward attribution is descriptive. A preceding preparation action is not proven necessary without a counterfactual.')


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root',type=Path,required=True)
    parser.add_argument('--config',type=Path,required=True)
    parser.add_argument('--chunks',type=int,nargs='+',required=True)
    parser.add_argument('--evaluations',type=int,nargs='+',required=True)
    parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args();config=read(args.config)
    require(not args.output.exists(),f'behavior output must be fresh: {args.output}')
    args.output.mkdir(parents=True)
    training=trajectories(args.root,args.chunks,config['strata'],args.output)
    result=dict(schema='declarative-behavior-analysis-v1',training=training,
        evaluation=[evaluation(args.root/f'evaluation/{n:04d}/report.json',config['strata']) for n in args.evaluations],
        input_root=str(args.root.resolve()),config=reference(args.config,args.config.parent))
    write(args.output/'report.json',result)
    print(dict(output=str(args.output),training_episodes=len(training['episodes']),
        successful_segments=sum(len(e['successes']) for e in training['episodes'])))


if __name__=='__main__':main()
