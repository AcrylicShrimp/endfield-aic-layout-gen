"""Read-only, case-separated outcome curves for the running on-policy study."""
import argparse
from collections import defaultdict
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import time

SOURCE_SHA256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest()

def mean(values):
    return sum(values)/len(values)


def aggregate(protocol, report):
    cases={c['id']:c for c in protocol['cases'] if c['id'] in protocol['training_cases']}
    episode_groups=defaultdict(list)
    for row in report['rows']:
        episode_groups[(row['initialization'],row['group'])].append(row)
    points=[]
    seen=set()
    elapsed=defaultdict(float)
    initial_scores={}
    next_group=defaultdict(int)
    for group in report['groups']:
        arm,index=group['initialization'],group['group']
        key=(arm,index)
        if key in seen or group['case'] not in cases or index!=next_group[arm]:
            raise ValueError('on-policy.monitor-group-membership')
        seen.add(key)
        next_group[arm]+=1
        rows=sorted(episode_groups[key],key=lambda r:r['attempt'])
        if (len(rows)!=protocol['config']['group_episodes'] or
            [r['attempt'] for r in rows]!=list(range(len(rows))) or
            any(r['case']!=group['case'] or not r['budget_complete'] or r['selected']!=protocol['config']['horizon'] for r in rows) or
            [r['best_metrics']['score'] for r in rows]!=group['scores']):
            raise ValueError(f'on-policy.monitor-group-episodes: {key}')
        scores={tuple(r['initial_metrics']['score']) for r in rows}
        if len(scores)!=1:
            raise ValueError('on-policy.monitor-reset-score')
        initial=next(iter(scores))
        if initial_scores.setdefault(group['case'],initial)!=initial:
            raise ValueError('on-policy.monitor-reset-drift')
        if any(tuple(r['best_metrics']['score'])>initial for r in rows):
            raise ValueError('on-policy.monitor-incumbent-regression')
        values=[100*(1-r['best_metrics']['score'][0]/initial[0]) for r in rows]
        points.append(dict(initialization=arm,case=group['case'],instances=cases[group['case']]['instances'],
            transfer_group=cases[group['case']]['transfer_group'],updates_before_sampling=index,
            completed_group_minutes_before_sampling=elapsed[arm]/60,episodes=len(rows),
            mean_area_reduction_percent=mean(values),min_area_reduction_percent=min(values),max_area_reduction_percent=max(values),
            mean_final_empty_xy_percent=mean([100*r['best_metrics']['empty_xy_cells']/r['best_metrics']['score'][0] for r in rows]),
            exact_score_improved_episodes=sum(tuple(r['best_metrics']['score'])<initial for r in rows),
            reported_improving_pose_actions=sum(r['improvements'].get('pose',0) for r in rows),
            reported_improving_reroute_actions=sum(r['improvements'].get('reroute',0) for r in rows),
            terminal_scores=group['scores'],reward_gradient_norm=group['reward_gradient_norm'],
            entropy_gradient_norm=group['entropy_gradient_norm']))
        elapsed[arm]+=group['seconds']
    curves=[]
    for arm in protocol['config']['initializations']:
        for case in protocol['training_cases']:
            selected=sorted([p for p in points if p['initialization']==arm and p['case']==case],key=lambda p:p['updates_before_sampling'])
            if not selected:
                continue
            curves.append(dict(initialization=arm,case=case,visits=len(selected),points=selected,
                latest_minus_first_mean_area_reduction_pp=selected[-1]['mean_area_reduction_percent']-selected[0]['mean_area_reduction_percent']))
    cycles=[]
    size=len(cases)
    for arm in protocol['config']['initializations']:
        selected=[p for p in points if p['initialization']==arm]
        for cycle in sorted({p['updates_before_sampling']//size for p in selected}):
            batch=[p for p in selected if p['updates_before_sampling']//size==cycle]
            if len(batch)!=size or {p['case'] for p in batch}!=set(cases):
                continue
            cycles.append(dict(initialization=arm,cycle=cycle,groups=len(batch),
                mean_area_reduction_percent=mean([p['mean_area_reduction_percent'] for p in batch]),
                by_transfer_group={group:mean([p['mean_area_reduction_percent'] for p in batch if p['transfer_group']==group])
                    for group in sorted({p['transfer_group'] for p in batch})}))
    return dict(completed_groups=len(points),completed_group_episodes=sum(p['episodes'] for p in points),
        pending_group_episodes=sum(len(v) for k,v in episode_groups.items() if k not in seen),curves=curves,balanced_complete_cycles=cycles)


def render(snapshot, directory):
    import matplotlib
    matplotlib.use('Agg')
    import matplotlib.pyplot as plt
    cases=snapshot['training_cases']
    pairs=[]
    for case in cases:
        base=case.removesuffix('--contraction-exhausted')
        if base not in pairs:
            pairs.append(base)
    fig,axes=plt.subplots(len(pairs),2,figsize=(13,2.45*len(pairs)),squeeze=False,layout='constrained')
    colors={'random':'#1769aa','imitation-warm':'#cb6500'}
    labels={'random':'Random initialization → RL','imitation-warm':'Imitation initialization → RL'}
    for row,base in enumerate(pairs):
        for column,suffix in enumerate(['','--contraction-exhausted']):
            ax=axes[row,column]
            case=base+suffix
            curves=[c for c in snapshot['curves'] if c['case']==case]
            high=0
            for curve in curves:
                points=curve['points'];arm=curve['initialization'];x=[p['updates_before_sampling'] for p in points]
                values=[p['mean_area_reduction_percent'] for p in points]
                ax.plot(x,values,'o-',color=colors[arm],label=labels[arm],linewidth=1.5,markersize=4)
                ax.fill_between(x,[p['min_area_reduction_percent'] for p in points],[p['max_area_reduction_percent'] for p in points],color=colors[arm],alpha=.15)
                high=max(high,max(p['max_area_reduction_percent'] for p in points))
            facility_count=curves[0]['points'][0]['instances'] if curves else snapshot['case_instances'][case]
            family=base.removeprefix(snapshot['case_families'][case]+'-').split('-varied-training-')[0].replace('-',' ').title()
            ax.set_title(f'{family} · {facility_count} facilities · {"Prepared" if column else "Original"}',fontsize=10,loc='left')
            ax.set_xlim(-1,snapshot['planned_groups']);ax.set_ylim(0,max(1,high*1.15))
            ax.set_ylabel('Area reduction (%)');ax.grid(alpha=.2)
            if row==len(pairs)-1:
                ax.set_xlabel('Policy updates before sampling')
            if not curves:
                ax.text(.5,.5,'No completed group yet',transform=ax.transAxes,ha='center',color='#777777')
    legends={label:handle for ax in axes.flat for handle,label in zip(*ax.get_legend_handles_labels())}
    if legends:
        fig.legend(legends.values(),legends.keys(),loc='outside lower center',ncol=2)
    fig.suptitle(f'On-policy training outcomes\nMean and observed range of {snapshot["group_episodes"]} episodes per fixed factory; not held-out evaluation',fontsize=13)
    for extension in ['png','svg']:
        temporary=directory/f'curves.tmp.{extension}'
        fig.savefig(temporary,dpi=140)
        temporary.replace(directory/f'curves.{extension}')
    plt.close(fig)


def read_snapshot(screen):
    if hashlib.sha256(Path(__file__).read_bytes()).hexdigest()!=SOURCE_SHA256:
        raise ValueError('on-policy.monitor-source-changed')
    protocol_file=screen/'protocol.json'
    protocol_bytes=protocol_file.read_bytes()
    protocol=json.loads(protocol_bytes)
    report_file=next((screen/name for name in ['training-report.json','partial.json','training-progress.json'] if (screen/name).exists()),None)
    if report_file is None:
        raise FileNotFoundError(f'on-policy.monitor-awaiting-progress: {screen}')
    report_bytes=report_file.read_bytes()
    report=json.loads(report_bytes)
    protocol_hash=hashlib.sha256(protocol_bytes).hexdigest()
    if report.get('protocol_sha256',protocol_hash)!=protocol_hash:
        raise ValueError('on-policy.monitor-protocol-identity')
    result=aggregate(protocol,report)
    result.update(schema_version=1,source_status=report['status'],source_file=str(report_file.resolve()),
        protocol_sha256=protocol_hash,source_sha256=hashlib.sha256(report_bytes).hexdigest(),
        training_cases=protocol['training_cases'],case_instances={c['id']:c['instances'] for c in protocol['cases']},
        case_families={c['id']:c['family'] for c in protocol['cases']},group_episodes=protocol['config']['group_episodes'],
        planned_groups=protocol['config']['groups'],generated_at=datetime.now(timezone.utc).isoformat(),
        measurement_scope='Exploratory on-policy training outcomes from recorded completed groups; independent physical audits and frozen evaluation remain required.',
        comparison_scope='Four changing-seed episodes per visit; first/latest differences are descriptive, not paired-seed tests or statistical significance.',
        memory_scope='This monitor reads reports only and performs no policy inference or environment actions.',
        time_scope='Group-time sums omit arm setup/finalization; update count is the primary horizontal coordinate.',
        monitor_source_sha256=SOURCE_SHA256)
    return result,report_file.name!='training-progress.json'


def main(args):
    if args.interval_seconds not in range(1,61) or args.max_seconds<=0:
        raise ValueError('on-policy.monitor-watch-bound')
    args.output.mkdir(parents=True,exist_ok=True)
    began=time.monotonic();previous=None
    while True:
        tick=time.monotonic()
        try:
            snapshot,terminal=read_snapshot(args.screen)
            signature=(snapshot['completed_groups'],snapshot['source_status'],terminal)
            if signature!=previous:
                render(snapshot,args.output)
                snapshot['monitor_refresh_seconds']=time.monotonic()-tick
                temporary=args.output/'snapshot.tmp.json'
                temporary.write_text(json.dumps(snapshot,indent=2)+'\n')
                temporary.replace(args.output/'snapshot.json')
                previous=signature
                print(json.dumps(dict(status='rendered',completed_groups=snapshot['completed_groups'],
                    pending_group_episodes=snapshot['pending_group_episodes'],terminal=terminal,seconds=snapshot['monitor_refresh_seconds'])),flush=True)
            if terminal or not args.watch:
                return
        except (FileNotFoundError,json.JSONDecodeError) as error:
            if not args.watch:
                raise
            print(json.dumps(dict(status='waiting',code='on-policy.monitor-source-unavailable',severity='info',stage='monitor',message=str(error))),flush=True)
        if time.monotonic()-began>=args.max_seconds:
            print(json.dumps(dict(status='stopped',code='on-policy.monitor-watch-bound',stage='monitor')),flush=True)
            return
        time.sleep(args.interval_seconds)


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--screen',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--watch',action='store_true')
    parser.add_argument('--interval-seconds',type=int,default=60)
    parser.add_argument('--max-seconds',type=int,default=18000)
    main(parser.parse_args())
