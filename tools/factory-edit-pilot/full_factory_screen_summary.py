"""Summarize the entire declared matrix without selecting a winning training seed."""
import argparse
from collections import Counter
import gzip
import json
from pathlib import Path
from statistics import mean


def unpack(path):
    with gzip.open(path,'rt') as stream:
        return json.load(stream)


def totals(rows):
    statuses,kinds,diagnostics=Counter(),Counter(),Counter()
    for r in rows:
        statuses.update(r['statuses']);kinds.update(r['kinds']);diagnostics.update(r['diagnostics'])
    return dict(episodes=len(rows),improved_episodes=sum(r['best_metrics']['score']<r['initial_metrics']['score'] for r in rows),
                area_improved_episodes=sum(r['best_metrics']['score'][0]<r['initial_metrics']['score'][0] for r in rows),
                mean_area_ratio=mean(r['best_metrics']['score'][0]/r['initial_metrics']['score'][0] for r in rows),
                selected=sum(r['selected'] for r in rows),native_proposals=sum(r['native_proposals'] for r in rows),
                route_expansions=sum(r['route_expansions'] for r in rows),unchanged=sum(r['unchanged'] for r in rows),
                revisited=sum(r['revisited'] for r in rows),statuses=dict(statuses),kinds=dict(kinds),diagnostics=dict(diagnostics))


def run(args):
    screen=json.loads((args.screen/'screen.json').read_text())
    rows=screen['rows'];evaluation=[r for r in rows if r['phase']=='evaluation']
    lookup={(r['arm'],r['case'],r['sample_seed']):r for r in evaluation}
    def paired(label,other):
        result=Counter(win=0,tie=0,loss=0)
        for row in evaluation:
            if row['arm']!=label:continue
            a=row['best_metrics']['score'];b=lookup[other,row['case'],row['sample_seed']]['best_metrics']['score']
            result['win' if a<b else 'loss' if a>b else 'tie']+=1
        return dict(result)
    arms={}
    for label in sorted({r['arm'] for r in evaluation}):
        current=[r for r in evaluation if r['arm']==label]
        result=totals(current)
        if label.endswith('-final'):
            result['paired_untrained']=paired(label,label.replace('-final','-untrained'))
            result['paired_uniform']=paired(label,'uniform')
        result['factory_sizes']={n:totals([r for r in current if r['instances']==n]) for n in sorted({r['instances'] for r in current})}
        result['heavy']=[{k:r[k] for k in ('case','sample_seed','initial_metrics','best_metrics')} for r in current if r['instances']==59]
        arms[label]=result
    kind_status=Counter();native_codes=Counter();move_sizes=Counter();pipe_edits=0
    for row in rows:
        episode=unpack(args.screen/row['directory']/'episode.json.gz')
        for t in episode['trace']:
            kind_status[t['action']['kind'],t['status']]+=1
            for d in t.get('acceptance',{}).get('diagnostics',[]):
                native_codes[d['message'].split(': ',1)[0]]+=1
            if not t['unchanged'] and t['status'] in ('improved','accepted_exploration','accepted_equal'):
                if t['action']['kind']=='move':move_sizes[row['instances']]+=1
                if t['action'].get('transport')=='pipe':pipe_edits+=1
    result=dict(schema_version=1,status=screen['status'],seconds=screen['seconds'],all=totals(rows),arms=arms,
                training=totals([r for r in rows if r['phase']=='training']),evaluation=totals(evaluation),
                rank_groups_nonzero=dict(Counter(g['arm'] for g in screen['groups'] if any(g['ranks']))),
                action_status=[dict(kind=k,status=s,count=n) for (k,s),n in sorted(kind_status.items())],
                native_diagnostics=dict(native_codes),accepted_changed_move_sizes=dict(move_sizes),
                accepted_changed_explicit_pipe_actions=pipe_edits)
    args.output.write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps(dict(status='complete',episodes=len(rows),arms=len(arms),output=str(args.output))))


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--screen',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True)
    run(parser.parse_args())
