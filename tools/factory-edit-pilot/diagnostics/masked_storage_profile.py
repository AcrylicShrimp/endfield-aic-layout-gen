"""Bounded storage attribution on existing data; never invoked by normal training."""
import argparse
import cProfile
import gzip
import json
from pathlib import Path
import pstats
import signal
import sys
import time


class CountBytes:
    def __init__(self): self.count = 0
    def write(self, value):
        size=memoryview(value).nbytes
        self.count += size
        return size
    def flush(self): pass


def measured(function, *args):
    profile = cProfile.Profile(); began = time.perf_counter()
    result = profile.runcall(function, *args)
    seconds = time.perf_counter()-began
    rows = []
    for (file,line,name),(primitive,calls,own,cumulative,_) in pstats.Stats(profile).stats.items():
        rows.append(dict(file=file,line=line,function=name,calls=calls,primitive_calls=primitive,
                         self_seconds=own,cumulative_seconds=cumulative))
    return result,dict(seconds=seconds,functions=sorted(rows,key=lambda r:r['cumulative_seconds'],reverse=True)[:35])


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source',type=Path,required=True)
    parser.add_argument('--branch',type=Path,required=True)
    parser.add_argument('--long-state',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args()
    assert not args.output.exists(), 'masked-profile.output: fresh diagnostic output required'
    args.output.mkdir(parents=True)
    signal.signal(signal.SIGALRM,lambda *_: (_ for _ in ()).throw(TimeoutError('masked-profile.deadline: 180 seconds')))
    signal.alarm(180)
    sys.path.insert(0,str(args.source/'tools/factory-edit-pilot'))
    import torch
    from blueprint_ppo_store import read,load,save,sha
    from ppo_tensor_pickle import Pickler
    torch.set_num_threads(1);torch.set_num_interop_threads(1)
    launch=read(args.branch/'launch.json');seed=launch['config']['seeds'][0];owner=args.branch/f'seed-{seed}'
    job=read(owner/'state.json')
    source=max((owner/v['path'] for v in job['replay_sources'].values()),key=lambda p:p.stat().st_size)
    payload,loading=measured(load,source)
    temporary=args.output/'storage-roundtrip.pt.gz'
    _,saving=measured(save,temporary,payload)
    assert temporary.stat().st_size < 64*1024**2,'masked-profile.storage: bounded disposable copy exceeded'
    saving['bytes']=temporary.stat().st_size;temporary.unlink()
    def serialized(value, compressed=False):
        counter=CountBytes()
        if compressed:
            with gzip.GzipFile(fileobj=counter,mode='wb',compresslevel=1) as stream:
                Pickler(stream,protocol=5).dump(value)
        else: Pickler(counter,protocol=5).dump(value)
        return counter.count
    samples=list(payload['samples'].values());fields={}
    for key in samples[0]:
        values=[s[key] for s in samples]
        began=time.perf_counter();raw=serialized(values);compressed=serialized(values,True)
        fields[key]=dict(pickle_bytes=raw,gzip_bytes=compressed,seconds=time.perf_counter()-began)
    inputs={}
    for key in samples[0]['input']:
        values=[s['input'][key] for s in samples]
        inputs[key]=dict(pickle_bytes=serialized(values),gzip_bytes=serialized(values,True))
    states=[]
    for path in (owner/'state.json',args.long_state):
        value=read(path)
        encoded,timing=measured(lambda:json.dumps(value,indent=2)+'\n')
        states.append(dict(path=str(path),update=value['update'],file_bytes=path.stat().st_size,
            encoded_bytes=len(encoded.encode()),input_rows=len(value.get('inputs',{})),profile=timing))
        del value,encoded
    result=dict(schema='masked-storage-profile-v1',status='complete',source=str(args.source),
        branch=str(args.branch),source_chunk=dict(path=str(source),sha256=sha(source),bytes=source.stat().st_size),
        samples=len(samples),loading=loading,saving=saving,sample_fields=fields,input_fields=inputs,
        completed=dict(count=len(payload['completed']),pickle_bytes=serialized(payload['completed']),
                       gzip_bytes=serialized(payload['completed'],True)),states=states,
        scope='One existing mature masked source; separate-field byte attribution is not additive. Profiling is excluded from normal-run speed comparisons.')
    (args.output/'profile.json').write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps({k:v for k,v in result.items() if k not in ('loading','saving','states')},indent=2))
    print('save_seconds',saving['seconds'],'load_seconds',loading['seconds'])
    print('state_seconds',[(v['update'],v['encoded_bytes'],v['profile']['seconds']) for v in states])
    signal.alarm(0)


if __name__=='__main__': main()
