"""Compare graph packing on identical retained rows; no training instrumentation."""
import argparse
from collections import Counter
import importlib.util
import math
from pathlib import Path
import signal
import sys
import time

sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
import torch
from blueprint_ppo_batch import pack
from blueprint_ppo_store import load,sha,write
from blueprint_ppo_update import digest,require
from declarative_learning_input import SCHEMA,PACKED


def compare(source, baseline, output):
    require(not output.exists(),'masked-pack-compare.output: fresh output required')
    signal.alarm(120);torch.set_num_threads(1);torch.set_num_interop_threads(1)
    spec=importlib.util.spec_from_file_location('baseline_pack',baseline)
    old=importlib.util.module_from_spec(spec);spec.loader.exec_module(old)
    payload=load(source);inputs=[s['input'] for s in payload['samples'].values()]
    began=time.perf_counter()
    for value in inputs:
        counts=Counter(a['kind'] for a in value['actions'])
        logs={kind:math.log(count) for kind,count in counts.items()}
        value['kind_log_counts']=torch.tensor([logs[a['kind']] for a in value['actions']],dtype=torch.float32)
    feature_seconds=time.perf_counter()-began
    batches=[inputs[i:i+64] for i in range(0,len(inputs),64)]
    for batch in batches:
        require(digest(old.pack(batch,schema=SCHEMA,packed_schema=PACKED))==
            digest(pack(batch,schema=SCHEMA,packed_schema=PACKED)),
            'masked-pack-compare.parity: packed tensor input changed')
    rows=[]
    for name,function in [('baseline',old.pack),('cached',pack)]*3:
        began=time.perf_counter()
        for batch in batches:function(batch,schema=SCHEMA,packed_schema=PACKED)
        rows.append(dict(kind=name,seconds=time.perf_counter()-began))
    result=dict(schema='masked-pack-comparison-v1',status='passed',samples=len(inputs),all_tensors_exact=True,
        source=dict(path=str(source),sha256=sha(source)),baseline=dict(path=str(baseline),sha256=sha(baseline)),
        feature_preparation_seconds=feature_seconds,rows=rows)
    write(output,result);signal.alarm(0);return result


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source',type=Path,required=True);parser.add_argument('--baseline',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args();print(compare(args.source,args.baseline,args.output))
