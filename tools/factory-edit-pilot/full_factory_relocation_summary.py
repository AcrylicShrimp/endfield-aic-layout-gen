"""Compare every relocation vocabulary at the same 32-action per-factory allowance."""
import argparse
from collections import Counter
import json
from pathlib import Path
from statistics import mean


def run(args):
    report=json.loads((args.relocation/'relocation.json').read_text())
    lookup={(c['arm'],c['case'],c['bank']):c for c in report['cells']}
    rowmap={r['directory']:r for r in report['rows']}
    def paired(label,other):
        scores=Counter(win=0,tie=0,loss=0);area=Counter(win=0,tie=0,loss=0)
        for cell in report['cells']:
            if cell['arm']!=label:continue
            a=cell['best_metrics']['score'];b=lookup[other,cell['case'],cell['bank']]['best_metrics']['score']
            scores['win' if a<b else 'loss' if a>b else 'tie']+=1
            area['win' if a[0]<b[0] else 'loss' if a[0]>b[0] else 'tie']+=1
        return dict(score=dict(scores),area=dict(area))
    def totals(cells):
        rows=[rowmap[n] for c in cells for n in c['attempts']]
        status,kinds,diag=Counter(),Counter(),Counter()
        for r in rows:status.update(r['statuses']);kinds.update(r['kinds']);diag.update(r['diagnostics'])
        return dict(cells=len(cells),episodes=len(rows),selected=sum(r['selected'] for r in rows),
                    improved=sum(c['best_metrics']['score']<c['initial_metrics']['score'] for c in cells),
                    area_improved=sum(c['best_metrics']['score'][0]<c['initial_metrics']['score'][0] for c in cells),
                    mean_area_reduction=mean(1-c['best_metrics']['score'][0]/c['initial_metrics']['score'][0] for c in cells),
                    physical_futile_fraction=mean(sum(rowmap[n]['physical_unchanged']+rowmap[n]['physical_revisited'] for n in c['attempts'])/c['selected'] for c in cells),
                    statuses=dict(status),kinds=dict(kinds),diagnostics=dict(diag),
                    native_proposals=sum(r['native_proposals'] for r in rows),route_expansions=sum(r['route_expansions'] for r in rows),
                    physical_unchanged=sum(r['physical_unchanged'] for r in rows),physical_revisited=sum(r['physical_revisited'] for r in rows),
                    legacy_unchanged=sum(r['unchanged'] for r in rows),legacy_revisited=sum(r['revisited'] for r in rows),
                    masked=sum(r['masked'] for r in rows),memory_exhausted=sum(r['memory_exhausted'] for r in rows))
    arms={}
    for label in sorted({c['arm'] for c in report['cells']}):
        cells=[c for c in report['cells'] if c['arm']==label];result=totals(cells)
        if label.endswith('-final'):result['vs_untrained']=paired(label,label.replace('-final','-untrained'))
        mode=label.rsplit('-',1)[0]
        if not label.endswith('-uniform'):result['vs_uniform']=paired(label,mode+'-uniform')
        if label!='local-final':result['vs_local_final']=paired(label,'local-final')
        result['sizes']={n:totals([c for c in cells if c['instances']==n]) for n in sorted({c['instances'] for c in cells})}
        result['heavy']=[{k:c[k] for k in ('case','bank','initial_metrics','best_metrics','best')} for c in cells if c['instances']==59]
        arms[label]=result
    bests={case:min((c for c in report['cells'] if c['case']==case),key=lambda c:c['best_metrics']['score'])
           for case in sorted({c['case'] for c in report['cells']})}
    result=dict(schema_version=1,status=report['status'],seconds=report['seconds'],arms=arms,best_by_input=bests,
                training_rank_groups=dict(Counter(g['arm'] for g in report['groups'] if any(g['ranks']))),
                selected_total=sum(r['selected'] for r in report['rows']),episodes=len(report['rows']),cells=len(report['cells']))
    args.output.write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps(dict(status='complete',arms=len(arms),episodes=len(report['rows']),cells=len(report['cells']))))


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--relocation',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True)
    run(parser.parse_args())
