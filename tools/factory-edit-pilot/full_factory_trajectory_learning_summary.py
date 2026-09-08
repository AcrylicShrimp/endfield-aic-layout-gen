"""Family-level descriptive comparisons; no checkpoint or hyperparameter selection."""
import argparse
from collections import Counter
import json
from pathlib import Path
import statistics


def summarize(directory):
    report=json.loads((directory/'report.json').read_text())
    if report['status']!='complete':raise ValueError('trajectory.summary-incomplete')
    rows=report['rows'];families=sorted({r['family'] for r in rows});table=[];paired=[];actions={}
    for method in ('untrained','learned','uniform','geometry'):
        selected=[r for r in rows if r['method']==method]
        kinds=Counter();statuses=Counter()
        for r in selected:kinds.update(r['kinds']);statuses.update(r['statuses'])
        actions[method]=dict(kinds=dict(kinds),statuses=dict(statuses))
        for family in families:
            group=[r for r in selected if r['family']==family]
            table.append(dict(family=family,method=method,episodes=len(group),
                area_improved=sum(r['best_metrics']['score'][0]<r['initial_metrics']['score'][0] for r in group),
                mean_area_reduction_percent=statistics.mean(100*(1-r['best_metrics']['score'][0]/r['initial_metrics']['score'][0]) for r in group),
                seconds=sum(r['seconds'] for r in group),shortened=sum(not r['budget_complete'] for r in group)))
    for method in ('untrained','uniform','geometry'):
        reference={(r['case'],r['bank']):r for r in rows if r['method']==method}
        for family in families:
            counts=Counter()
            for r in rows:
                if r['method']!='learned' or r['family']!=family:continue
                a,b=r['best_metrics']['score'],reference[r['case'],r['bank']]['best_metrics']['score']
                counts['wins' if a<b else 'losses' if a>b else 'ties']+=1
            paired.append(dict(family=family,against=method,**{k:counts[k] for k in ('wins','ties','losses')}))
    labels=json.loads((directory/'label-scores.json').read_text());label_groups=[]
    for phase in ('untrained','learned'):
        for split in ('train','development'):
            group=[r for r in labels if r['phase']==phase and r['split']==split]
            label_groups.append(dict(phase=phase,split=split,lessons=len(group),top1=sum(r['top1'] for r in group),
                                     mean_teacher_probability=statistics.mean(r['probability'] for r in group)))
    return dict(schema_version=1,scope='Descriptive opened-family comparisons; paired complete lexicographic scores',
        seconds=report['seconds'],cache_array_bytes_peak=report['cache_array_bytes_peak'],selected=sum(r['selected'] for r in rows),
        families=table,paired=paired,actions=actions,labels=label_groups)


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--screen',type=Path,required=True);p.add_argument('--output',type=Path,required=True)
    a=p.parse_args();result=summarize(a.screen)
    with a.output.open('x') as stream:json.dump(result,stream,indent=2);stream.write('\n')
    print(json.dumps(result))
