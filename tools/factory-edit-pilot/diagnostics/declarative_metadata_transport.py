"""Bounded same-value old/new response restoration with ordinary GC enabled."""
import argparse
from collections.abc import Sequence
import gc
from pathlib import Path
import sys
import time

BASE=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(BASE))
import torch
from blueprint_ppo_store import read,write
from blueprint_ppo_update import require
from declarative_learning_environment import Runtime,Stream
from declarative_native_input import PortFeatures
import declarative_learning_policy as policy
from ppo_harness_transport import Reader,encode


class PreviousPortGroups(Sequence):
    """Exact pre-sharing representation; no new metadata encoder in its decode."""
    def __init__(self,metadata,bank):
        self.metadata,self.bank=metadata,bank;self.cache={}
    def __len__(self):return len(self.metadata)
    def __getitem__(self,owner):
        if isinstance(owner,slice):return [self[i] for i in range(*owner.indices(len(self)))]
        owner=int(owner)
        if owner not in self.cache:
            self.cache[owner]=[dict(group,features=PortFeatures(group['features'],self.bank)) for group in self.metadata[owner]]
        return self.cache[owner]
    def __reduce__(self):return type(self),(self.metadata,self.bank)


def run(launch_path,output,messages):
    require(not output.exists() and 0<messages<=512,'metadata-transport.bounds: fresh receipt and 1..512 messages required')
    torch.set_num_threads(1);torch.use_deterministic_algorithms(True)
    launch=read(launch_path);config=launch['config']
    require(launch['masked_long']['probe'] and config['training']['streams']==128,'metadata-transport.scope: 128-stream probe required')
    runtime=Runtime(config,launch['source']['native_library']);new=[];previous=[]
    for index in range(0,128,16):
        stream=Stream(runtime,index);stream.observe();_,prepared=stream.runtime.observe(stream.session)
        new.append(prepared);group=prepared['port_groups']
        previous.append(dict(prepared,port_groups=PreviousPortGroups(group.metadata,group.bank)))
    model,_,_=policy.learners(17,config['ppo'])
    with torch.no_grad():
        expected=policy.sample(model,new,torch.Generator().manual_seed(51))
        require(expected==policy.sample(model,previous,torch.Generator().manual_seed(51)),'metadata-transport.values: source sampling differs')
    rows=[]
    for mode,inputs in (('previous',previous),('shared',new),('shared',new),('previous',previous)):
        gc.collect();reader=Reader(512*1024**2,owned_buffers=True);live=[];starts={};gcs={}
        def callback(event,info):
            generation=info['generation']
            if event=='start':starts[generation]=time.perf_counter()
            elif generation in starts:
                row=gcs.setdefault(generation,dict(calls=0,seconds=0.,collected=0))
                row['calls']+=1;row['seconds']+=time.perf_counter()-starts.pop(generation);row['collected']+=info['collected']
        gc.callbacks.append(callback);encoding=decoding=0.
        descriptors_before=len(list(Path('/proc/self/fd').iterdir()))
        try:
            for _ in range(messages):
                began=time.perf_counter();frame,fd=encode(dict(inputs=inputs));encoding+=time.perf_counter()-began
                began=time.perf_counter();live.extend(reader.feed(frame,[fd]));decoding+=time.perf_counter()-began
        finally:gc.callbacks.remove(callback);reader.close()
        require(len(list(Path('/proc/self/fd').iterdir()))<=descriptors_before+1,'metadata-transport.descriptors: persistent fd leak')
        with torch.no_grad():require(expected==policy.sample(model,live[-1]['inputs'],torch.Generator().manual_seed(51)),
            'metadata-transport.values: restored sampling differs')
        rows.append(dict(mode=mode,messages=messages,encode_seconds=encoding,decode_seconds=decoding,gc=gcs,
            shared_bytes=reader.shared_bytes,metadata_bytes=reader.bytes))
        del live
    result=dict(schema='declarative-metadata-transport-v1',status='passed',launch=str(launch_path),gc_policy_unchanged=True,rows=rows)
    write(output,result);print(result,flush=True)


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--launch',type=Path,required=True)
    p.add_argument('--output',type=Path,required=True);p.add_argument('--messages',type=int,default=512)
    a=p.parse_args();run(a.launch.resolve(),a.output.resolve(),a.messages)
