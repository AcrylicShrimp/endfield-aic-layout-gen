"""Separate constructor attrition from frozen policy outcomes on fresh requests."""
import argparse
from collections import Counter
import json
from pathlib import Path

from full_factory_screen import sha,write


def summarize(directory):
    p=json.loads((directory/'protocol.json').read_text());r=json.loads((directory/'report.json').read_text())
    a=json.loads(Path(p['admission']).read_text())
    if r['status']!='complete' or r['protocol_sha256']!=sha(directory/'protocol.json'):raise ValueError('transfer-summary.incomplete')
    attempts=[]
    for row in a['rows']:
        detail={k:row.get(k) for k in ('id','family','split','status','factory_validated','facilities','covered_facilities',
                                      'size_stratum','construction_ms','internal_pipe_requirements','topology','diagnostics')}
        if row['status']=='construction-stopped':
            construction_path=Path(row['directory'])/'construction.json';wiring_path=Path(row['directory'])/'wiring.json'
            c=json.loads(construction_path.read_text());wiring=json.loads(wiring_path.read_text())
            nodes={n['id']:n for n in wiring['nodes']};edges={e['id']:e for e in wiring['edges']}
            failures=[]
            for reason,count in c['rejected_reasons'].items():
                if not reason.startswith('connection failed: '):continue
                edge=edges[reason.removeprefix('connection failed: ')];failures.append(dict(edge,count=count,
                    source_recipe=nodes[edge['source']].get('recipe'),target_recipe=nodes[edge['target']].get('recipe')))
            detail.update(failed_placement_attempts=c['failed_placement_attempts'],failed_statistics=c['failed_statistics'],
                per_connection_route_limit=c['per_connection_route_limit'],routing_workspace_cell_limit=c['routing_workspace_cell_limit'],
                rejected_reasons=c['rejected_reasons'],failed_connections=sorted(failures,key=lambda x:-x['count']),
                attempted_groups=[[dict(instance=i,recipe=nodes[i].get('recipe')) for i in group] for group in c['attempted_groups_at_failure']],
                construction_sha256=sha(construction_path),wiring_sha256=sha(wiring_path))
        attempts.append(detail)
    comparisons=[]
    for row in r['rows']:
        if row['method']=='geometry':continue
        for reference in ('parent','geometry'):
            if reference==row['method']:continue
            other=next(x for x in r['rows'] if x['method']==reference and x['case']==row['case'] and x['bank']==row['bank'])
            if row['initial_metrics']!=other['initial_metrics']:raise ValueError('transfer-summary.initial')
            x,y=row['best_metrics']['score'],other['best_metrics']['score']
            comparisons.append(dict(method=row['method'],reference=reference,case=row['case'],bank=row['bank'],instances=row['instances'],
                outcome='win' if x<y else 'loss' if x>y else 'tie',both_budget_complete=row['budget_complete'] and other['budget_complete']))
    rows=[dict(x,area_reduction_percent=100*(1-x['best_metrics']['score'][0]/x['initial_metrics']['score'][0])) for x in r['rows']]
    return dict(schema_version=1,scope='Fresh-size development within the trained family; construction attrition remains separate from policy quality',
        attempted=a['attempted'],accepted=a['accepted'],admission_seconds=a['elapsed_ms']/1000,evaluation_seconds=r['seconds'],
        admitted_65_to_128=sum(x['factory_validated'] and 65<=x.get('facilities',0)<=128 for x in a['rows']),
        statuses=dict(Counter(x['status'] for x in a['rows'])),attempts=attempts,rows=rows,comparisons=comparisons,
        provenance={str(f.resolve()):sha(f) for f in (directory/'protocol.json',directory/'report.json',Path(p['admission']))})


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--screen',type=Path,required=True);parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args();result=summarize(args.screen);write(args.output,result)
    print(json.dumps({k:result[k] for k in ('attempted','accepted','admitted_65_to_128','statuses','evaluation_seconds')}))
