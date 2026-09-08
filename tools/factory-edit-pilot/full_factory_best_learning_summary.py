"""Opened-family full-score learning outcomes with explicitly reused controls."""
import argparse
from collections import Counter
import json
from pathlib import Path
import statistics

from full_factory_screen import sha,write


def summarize(directory,decoding,ranking):
    p=json.loads((directory/'protocol.json').read_text());r=json.loads((directory/'report.json').read_text())
    parent=json.loads((Path(p['parent'])/'report.json').read_text());decoded=json.loads((decoding/'report.json').read_text())
    binary=json.loads((ranking/'report.json').read_text());dp=json.loads((decoding/'protocol.json').read_text())
    bp=json.loads((ranking/'protocol.json').read_text())
    if any(x['status']!='complete' for x in (r,parent,decoded,binary)):raise ValueError('best-summary.incomplete')
    if dp['parent_protocol_sha256']!=parent['protocol_sha256'] or dp['checkpoint_sha256']!=sha(Path(p['parent'])/'learned.pt'):
        raise ValueError('best-summary.decoding-parent')
    if bp['sources'][str((Path(p['parent'])/'learned.pt').resolve())]!=sha(Path(p['parent'])/'learned.pt'):
        raise ValueError('best-summary.ranking-parent')
    original=json.loads((Path(p['parent'])/'protocol.json').read_text())
    for control in (dp,bp,original['plan']):
        if any(control[k]!=p[k] for k in ('horizon','banks','episode_seconds','route_work')):
            raise ValueError('best-summary.control-budget')
    methods={f'{arm}-t{t}':[x for x in r['rows'] if x['arm']==arm and x['temperature']==t] for arm in p['arms'] for t in p['temperatures']}
    methods.update({f'binary-t{t}':[x for x in binary['rows'] if x['temperature']==t] for t in p['temperatures']})
    methods.update({'parent-t1':[x for x in parent['rows'] if x['method']=='learned'],
                    'parent-t0.05':[x for x in decoded['rows'] if x['temperature']==.05],
                    'geometry':[x for x in parent['rows'] if x['method']=='geometry']})
    expected={(c['id'],bank) for c in p['cases'] for bank in p['banks']};tables=[];actions={};paired=[]
    for method,rows in methods.items():
        if len(rows)!=len(expected) or {(x['case'],x['bank']) for x in rows}!=expected:raise ValueError('best-summary.matrix')
        kinds=Counter();statuses=Counter();improvements=Counter()
        for row in rows:
            if row['selected']>p['horizon']:raise ValueError('best-summary.selection-bound')
            kinds.update(row['kinds']);statuses.update(row['statuses']);improvements.update(row.get('improvements',{}))
        actions[method]=dict(kinds=dict(kinds),statuses=dict(statuses),improving_kinds=dict(improvements) if rows[0].get('improvements') is not None else None)
        for family in sorted({x['family'] for x in rows}):
            group=[x for x in rows if x['family']==family]
            tables.append(dict(method=method,family=family,episodes=len(group),
                mean_area_reduction_percent=statistics.mean(100*(1-x['best_metrics']['score'][0]/x['initial_metrics']['score'][0]) for x in group),
                shortened=sum(not x['budget_complete'] for x in group)))
    for arm in p['arms']:
        for t in p['temperatures']:
            method=f'{arm}-t{t}'
            for reference in (f'parent-t{t}',f'binary-t{t}','geometry'):
                lookup={(x['case'],x['bank']):x for x in methods[reference]}
                for family in sorted({x['family'] for x in methods[method]}):
                    counts=Counter()
                    for row in methods[method]:
                        if row['family']!=family:continue
                        other=lookup[row['case'],row['bank']]
                        if row['initial_metrics']!=other['initial_metrics']:raise ValueError('best-summary.initial')
                        a,b=row['best_metrics']['score'],other['best_metrics']['score']
                        counts['wins' if a<b else 'losses' if a>b else 'ties']+=1
                    paired.append(dict(method=method,reference=reference,family=family,**{k:counts[k] for k in ('wins','ties','losses')}))
    label_rows=json.loads((directory/'label-scores.json').read_text());labels=[]
    for phase in ('parent',*p['arms']):
        for split in ('train','development'):
            group=[x for x in label_rows if x['phase']==phase and x['split']==split and x['teacher_present']]
            labels.append(dict(phase=phase,split=split,teachers=len(group),top1=sum(x['top1'] for x in group),
                mean_target_mass=statistics.mean(x['target_mass'] for x in group)))
    return dict(schema_version=1,scope='Adaptive opened-family comparison; reused parent/binary/geometry controls, no sealed evaluation',
        seconds=r['seconds'],selected=sum(x['selected'] for x in r['rows']),labels=labels,families=tables,paired=paired,actions=actions,
        provenance={str(f.resolve()):sha(f) for f in (directory/'report.json',Path(p['parent'])/'report.json',decoding/'report.json',decoding/'protocol.json',ranking/'report.json',ranking/'protocol.json')})


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    for name in ('screen','decoding','ranking','output'):parser.add_argument('--'+name,type=Path,required=True)
    args=parser.parse_args();result=summarize(args.screen,args.decoding,args.ranking);write(args.output,result);print(json.dumps(result))
