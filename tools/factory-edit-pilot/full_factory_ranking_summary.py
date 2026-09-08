"""Descriptive ranking-label and frozen original-factory comparisons."""
import argparse
from collections import Counter
import json
from pathlib import Path
import statistics

from full_factory_screen import sha,write


def summarize(directory,decoding):
    p=json.loads((directory/'protocol.json').read_text());r=json.loads((directory/'report.json').read_text())
    parent=json.loads((Path(p['parent'])/'report.json').read_text());decoded=json.loads((decoding/'report.json').read_text())
    dp=json.loads((decoding/'protocol.json').read_text())
    if any(x['status']!='complete' for x in (r,parent,decoded)):raise ValueError('ranking.summary-incomplete')
    if dp['parent_protocol_sha256']!=parent['protocol_sha256'] or dp['checkpoint_sha256']!=sha(Path(p['parent'])/'learned.pt'):
        raise ValueError('ranking.summary-decoding-parent')
    labels=json.loads((Path(p['collection'])/'contrasts.json').read_text())['rows']
    scores={(s['phase'],s['id']):s for s in json.loads((directory/'label-scores.json').read_text())}
    observations=[]
    for phase in ('parent','ranked'):
        for split in ('train','development'):
            for kind in sorted({x['action']['kind'] for x in labels if x['improving'] and x['split']==split}):
                positives=[x for x in labels if x['split']==split and x['completed'] and x['improving'] and x['action']['kind']==kind]
                pairs=[(a,b) for a in positives for b in labels if a['root']==b['root'] and b['completed'] and not b['improving']]
                counts=Counter('wins' if scores[phase,a['id']]['logit']>scores[phase,b['id']]['logit'] else
                               'losses' if scores[phase,a['id']]['logit']<scores[phase,b['id']]['logit'] else 'ties' for a,b in pairs)
                observations.append(dict(phase=phase,split=split,kind=kind,positives=len(positives),pairs=len(pairs),**counts))
    methods={f'ranked-t{t}':[x for x in r['rows'] if x['temperature']==t] for t in p['temperatures']}
    methods.update({'parent-t1':[x for x in parent['rows'] if x['method']=='learned'],
                    'parent-t0.05':[x for x in decoded['rows'] if x['temperature']==.05],
                    'geometry':[x for x in parent['rows'] if x['method']=='geometry']})
    expected={(c['id'],bank) for c in p['cases'] for bank in p['banks']}
    tables=[];actions={};paired=[]
    for method,rows in methods.items():
        if len(rows)!=len(expected) or {(x['case'],x['bank']) for x in rows}!=expected:raise ValueError('ranking.summary-matrix')
        kinds=Counter();statuses=Counter()
        for row in rows:kinds.update(row['kinds']);statuses.update(row['statuses'])
        actions[method]=dict(kinds=dict(kinds),statuses=dict(statuses))
        for family in sorted({x['family'] for x in rows}):
            group=[x for x in rows if x['family']==family]
            tables.append(dict(method=method,family=family,episodes=len(group),
                mean_area_reduction_percent=statistics.mean(100*(1-x['best_metrics']['score'][0]/x['initial_metrics']['score'][0]) for x in group),
                shortened=sum(not x['budget_complete'] for x in group)))
    for t in p['temperatures']:
        method=f'ranked-t{t}'
        for reference in (f'parent-t{t}','geometry'):
            lookup={(x['case'],x['bank']):x for x in methods[reference]}
            for family in sorted({x['family'] for x in methods[method]}):
                counts=Counter()
                for row in methods[method]:
                    if row['family']!=family:continue
                    other=lookup[row['case'],row['bank']]
                    if row['initial_metrics']!=other['initial_metrics']:raise ValueError('ranking.summary-initial')
                    a,b=row['best_metrics']['score'],other['best_metrics']['score']
                    counts['wins' if a<b else 'losses' if a>b else 'ties']+=1
                paired.append(dict(method=method,reference=reference,family=family,**{k:counts[k] for k in ('wins','ties','losses')}))
    return dict(schema_version=1,scope='Opened-family descriptive comparisons with reused frozen parent controls; no sealed evaluation',
        seconds=r['seconds'],selected=sum(x['selected'] for x in r['rows']),labels=observations,families=tables,paired=paired,actions=actions,
        provenance={str(f.resolve()):sha(f) for f in (directory/'report.json',Path(p['parent'])/'report.json',decoding/'report.json',decoding/'protocol.json')})


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    for name in ('screen','decoding','output'):parser.add_argument('--'+name,type=Path,required=True)
    args=parser.parse_args();result=summarize(args.screen,args.decoding);write(args.output,result);print(json.dumps(result))
