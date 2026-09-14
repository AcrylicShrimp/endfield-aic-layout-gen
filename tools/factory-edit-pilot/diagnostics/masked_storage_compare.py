"""Bounded same-data codec/loader comparison; no normal-run instrumentation."""
import argparse
import gc
import gzip
from pathlib import Path
import signal
import sys
import time

sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
import torch
import ppo_tensor_pickle
from blueprint_ppo_store import load,save,sha,write
from blueprint_ppo_update import digest,require
from declarative_replay_storage import compact
import declarative_learning_policy as policy


def legacy_save(path, value):
    with gzip.open(path,'wb',compresslevel=1) as stream:
        torch.save(value,stream,pickle_module=ppo_tensor_pickle,pickle_protocol=5)


def compare(source, output):
    require(not output.exists(),'masked-storage-compare.output: fresh output required')
    output.mkdir(parents=True);signal.alarm(180)
    torch.set_num_threads(1);torch.set_num_interop_threads(1)
    original=load(source);rows=[]
    selected=compact(original,set(original['samples']))
    for name,value,writer in (('torch-wrapper',original,legacy_save),
            ('direct-full',original,save),('direct-selected-branches',selected,save)):
        gc.collect();path=output/f'{name}.pt.gz'
        began=time.perf_counter();writer(path,value);saving=time.perf_counter()-began
        began=time.perf_counter();restored=load(path);loading=time.perf_counter()-began
        size=path.stat().st_size
        require(size<64*1024**2,'masked-storage-compare.output: disposable cell exceeds bound')
        if name=='direct-selected-branches':
            indices=list(original['samples'])
            for start in range(0,len(indices),64):
                old=[original['samples'][i] for i in indices[start:start+64]]
                new=[restored['samples'][i] for i in indices[start:start+64]]
                require(digest(policy.prepare_rows(old))==digest(policy.prepare_rows(new)),
                    'masked-storage-compare.likelihood: changed numeric root or conditional domain')
        elif name=='direct-full':
            require(digest(original)==digest(restored),'masked-storage-compare.roundtrip: changed full payload')
        rows.append(dict(encoding=name,save_seconds=saving,load_seconds=loading,bytes=size))
        path.unlink();del restored
    result=dict(schema='masked-storage-comparison-v1',status='passed',source=dict(path=str(source),sha256=sha(source)),
        samples=len(original['samples']),rows=rows,all_recorded_likelihood_inputs_equal=True,
        scope='One unprofiled pass per encoding on identical retained data; disposable archives removed.')
    write(output/'report.json',result);signal.alarm(0);return result


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source',type=Path,required=True);parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args();print(compare(args.source,args.output))
