"""Replay one recorded constructor request with an isolated read-only trace build."""
import argparse
from collections import Counter
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time


def read(path):
    return json.loads(Path(path).read_text())


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def stable(value):
    """Construction timing is observational; all other recorded fields must match."""
    if isinstance(value,dict):return {k:stable(v) for k,v in value.items() if k!='elapsed_ms'}
    if isinstance(value,list):return [stable(v) for v in value]
    return value


def run(args):
    attempt=args.attempt.resolve();baseline=read(attempt/'construction.json');process=read(attempt/'construction-process.json')
    admission=read(attempt.parent/'report.json');edge_ids={e['id'] for e in read(attempt/'wiring.json')['edges']}
    if args.edge not in edge_ids:raise ValueError(f'attachment.unknown-edge: {args.edge}')
    for source in admission['provenance']:
        if sha(source['path'])!=source['sha256']:raise ValueError(f'attachment.baseline-input: {source["path"]}')
    args.output.mkdir();output=args.output.resolve();config=output/'trace-config.json'
    config.write_text(json.dumps(dict(edge=args.edge,output=str(output/'trials.jsonl'),snapshot=str(output/'snapshot.json'),max_rows=4096),indent=2)+'\n')
    argv=list(process['argv'])
    for flag,name in [('--report-output','construction.json'),('--visualization-output','construction.html')]:
        argv[argv.index(flag)+1]=str(output/name)
    sources={str(p.resolve()):sha(p) for p in [attempt/'construction.json',attempt/'construction-process.json',attempt/'wiring.json',
        attempt/'source-plan.json',args.binary,args.patch,Path(__file__),config]}
    protocol=dict(schema_version=1,attempt=str(attempt),edge=args.edge,binary=str(args.binary.resolve()),argv=argv,
        seconds=60,source_sha256=sources,baseline_provenance=admission['provenance'])
    (output/'protocol.json').write_text(json.dumps(protocol,indent=2)+'\n')
    env=dict(os.environ,AIC_ATTACHMENT_DIAGNOSTIC=str(config));began=time.perf_counter();timed_out=False
    with (output/'process.stdout').open('wb') as stdout,(output/'process.stderr').open('wb') as stderr:
        try:
            result=subprocess.run([str(args.binary.resolve()),*argv],env=env,stdout=stdout,stderr=stderr,timeout=60,check=False)
            code=result.returncode
        except subprocess.TimeoutExpired:
            timed_out=True;code=None
    (output/'process.json').write_text(json.dumps(dict(code=code,timed_out=timed_out,seconds=time.perf_counter()-began),indent=2)+'\n')
    if timed_out:raise TimeoutError('attachment.diagnostic-child-budget')
    if code!=process['code']:raise ValueError(f'attachment.exit-mismatch: expected {process["code"]}, observed {code}; see {output/"process.stderr"}')
    actual=read(output/'construction.json');matches=stable(actual)==stable(baseline)
    trials=[json.loads(line) for line in (output/'trials.jsonl').read_text().splitlines()]
    if not trials or len(trials)>4096:raise ValueError('attachment.trace-membership')
    reasons=Counter();pairs=Counter();lanes=Counter()
    for i,row in enumerate(trials):
        if row['ordinal']!=i or row['edge']!=args.edge:raise ValueError('attachment.trace-order')
        pairs[row['source']['attachment']['kind']+' -> '+row['target']['attachment']['kind']]+=1;lanes[row['lane']]+=1
        if row['path_found']:reasons['path-found']+=1;continue
        causes=[]
        for side in ('source','target'):
            s=row[side]
            for key in ('reservation_only','facility_blocked','layer_blocked','crossing'):
                if s[key]:causes.append(side+'-'+key)
            if not s['inside']:causes.append(side+'-outside')
        reasons[';'.join(causes) if causes else 'no-path-with-free-endpoints']+=1
    for p,h in sources.items():
        if sha(p)!=h:raise ValueError(f'attachment.changed-input: {p}')
    report=dict(schema_version=1,status='passed' if matches else 'baseline-mismatch',baseline_reproduced=matches,
        seconds=time.perf_counter()-began,trials=len(trials),reasons=dict(reasons),pairs=dict(pairs),lanes=dict(lanes),
        scope='Exact non-timing constructor replay; endpoint observations for one selected connection, no intervention',
        protocol_sha256=sha(output/'protocol.json'),trial_sha256=sha(output/'trials.jsonl'),snapshot_sha256=sha(output/'snapshot.json'))
    (output/'report.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report))
    if not matches:raise ValueError('attachment.baseline-replay-mismatch')


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__)
    for name in ('attempt','binary','patch','output'):p.add_argument('--'+name,type=Path,required=True)
    p.add_argument('--edge',required=True);run(p.parse_args())
