"""Separate original-root learning, inverse-task transfer and generator discoveries."""
import argparse
from collections import Counter
import json
from pathlib import Path
from statistics import mean


def run(args):
    r=json.loads((args.screen/'screen.json').read_text());lookup={(c['arm'],c['case'],c['bank']):c for c in r['cells']};rows={row['directory']:row for row in r['rows']}
    def paired(cells,other):
        return dict(Counter('win' if c['best_metrics']['score']<lookup[other,c['case'],c['bank']]['best_metrics']['score'] else
                            'loss' if c['best_metrics']['score']>lookup[other,c['case'],c['bank']]['best_metrics']['score'] else 'tie' for c in cells))
    arms={}
    for arm in sorted({c['arm'] for c in r['cells']}):
        cells=[c for c in r['cells'] if c['arm']==arm];episodes=[rows[d] for c in cells for d in c['attempts']]
        result=dict(cells=len(cells),improved=sum(c['best_metrics']['score']<c['initial_metrics']['score'] for c in cells),area_improved=sum(c['best_metrics']['score'][0]<c['initial_metrics']['score'][0]for c in cells),
                    mean_area_reduction=mean(1-c['best_metrics']['score'][0]/c['initial_metrics']['score'][0]for c in cells),
                    futile_fraction=sum(e['physical_unchanged']+e['physical_revisited']for e in episodes)/sum(e['selected']for e in episodes),
                    root_pairs=paired(cells,'untrained-d2' if 'd2' in arm else 'untrained-d0'),uniform_pairs=paired(cells,'uniform'),
                    heavy=[c for c in cells if c['instances']==59])
        if arm=='mixed-pg-final':result['vs_root_pg']=paired(cells,'root-pg-final')
        result['inverse']={}
        for selection in ('sample','greedy'):
            v=[x for x in r['inverse'] if x['arm']==arm and x['selection']==selection]
            result['inverse'][selection]=dict(tasks=len(v),improved=sum(x['status']=='improved' for x in v),teacher_chosen=sum(x['teacher_chosen']for x in v),
                                              mean_area_reduction=mean(1-x['best_metrics']['score'][0]/x['initial_metrics']['score'][0]for x in v),
                                              roots={root:dict(Counter(x['status']for x in v if x['root']==root))for root in sorted({x['root']for x in v})})
        arms[arm]=result
    result=dict(status=r['status'],seconds=r['seconds'],arms=arms,
                best_by_input={case:min((c for c in r['cells']if c['case']==case),key=lambda c:c['best_metrics']['score'])for case in sorted({c['case']for c in r['cells']})},
                training={a:json.loads((args.screen/f'{a}-training.json').read_text())for a in ('root-pg','mixed-pg','inverse-d0','inverse-d2')})
    args.output.write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(dict(status='complete',arms=len(arms))))


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--screen',type=Path,required=True);p.add_argument('--output',type=Path,required=True);run(p.parse_args())
