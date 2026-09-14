"""Exact CUDA transitions on identical recorded native fixture inputs, without recollection."""
import argparse
from copy import deepcopy
import json
import os
from pathlib import Path
import sys
import time

sys.path[:0]=[str(Path(__file__).resolve().parent),str(Path(__file__).resolve().parents[1])]
from declarative_quality_branch import PROFILES,profile_success,source_binding
from declarative_quality_observer import Observer,quartiles
from blueprint_ppo_store import sha,write
from blueprint_ppo_update import digest,require


def verify(output,library):
    import torch
    import declarative_learning_policy as policy
    from declarative_learning_runtime import Learner
    from test_declarative_replay_sampling import NativeSamplingTests
    from test_declarative_learning import MemoryEvidence
    os.environ['AIC_NATIVE_LIBRARY']=str(library)
    os.environ.setdefault('CUBLAS_WORKSPACE_CONFIG',':4096:8')
    torch.set_num_threads(1);torch.set_num_interop_threads(1);torch.use_deterministic_algorithms(True)
    require(torch.cuda.is_available(),'quality-study.cuda-proof: CUDA required')
    output=Path(output);require(not output.exists(),'quality-study.cuda-proof: fresh proof path required')
    began=time.monotonic();helper,config,chunks,evidence,saved=NativeSamplingTests().prepared()
    config['device']='cuda:0';saved['device']='cuda:0';results=[]
    def behavior_payload(learner,chunk):
        payload=deepcopy(chunk)
        with torch.no_grad():
            lp,ent,values=policy.predict(learner.model,policy.device_rows(policy.prepare_rows(payload['samples']),'cuda:0'))
        for r,l,e,v in zip(payload['samples'],lp.cpu().tolist(),ent.cpu().tolist(),values.cpu().tolist(),strict=True):
            r.update(old_logp=l,old_value=v,entropy=e)
        payload['model_before']=digest(learner.model.state_dict());return payload
    for profile in PROFILES:
        require(time.monotonic()-began<120,'quality-study.cuda-proof: fixture time ceiling')
        selected=deepcopy(config);selected['success']=profile_success(config['success'],profile)
        warm=Learner(17,selected,'sampling-test',saved);source=MemoryEvidence();source.chunks=deepcopy(evidence.chunks)
        require(warm.actor.state and warm.critic.state,'quality-study.cuda-proof: nonempty Adam required')
        warm.update(behavior_payload(warm,chunks[0]),source)
        checkpoint=deepcopy(warm.checkpoint([]));payload=behavior_payload(warm,chunks[1]);payload_hash=digest(payload)
        before_source=deepcopy(source.chunks);rows=[]
        for kind in ('warm','restored','restored-observed'):
            learner=warm if kind=='warm' else Learner(17,selected,'sampling-test',checkpoint)
            require(next(learner.model.parameters()).device==torch.device('cuda:0'),'quality-study.cuda-proof: wrong learner device')
            require(digest(learner.checkpoint([]))==digest(checkpoint),'quality-study.cuda-proof: initial restore differs')
            current=MemoryEvidence();current.chunks=deepcopy(before_source)
            observer=Observer(learner,quartiles(checkpoint['buffer']['entries'],selected['strata'])) if kind=='restored-observed' else None
            if observer:observer.begin()
            supplied=deepcopy(payload);require(digest(supplied)==payload_hash,'quality-study.cuda-proof: different full update inputs')
            record=learner.update(supplied,current)
            if observer:observer.finish(record)
            state=deepcopy(learner.checkpoint([]))
            # New native-quality annotations are the sole observer-owned metadata.
            for e in state['buffer']['entries'].values():e.pop('retention_features_v1',None)
            ppo=deepcopy(record['ppo']);ppo.pop('execution')
            rows.append(dict(state_sha256=digest(state),events_sha256=digest(record['events']),ppo_sha256=digest(ppo)))
        require(rows[0]==rows[1]==rows[2],f'quality-study.cuda-proof: transition differs for {profile}')
        results.append(dict(profile=profile,input_sha256=payload_hash,**rows[0],nonempty_adam=True,device='cuda:0'))
    result=dict(schema='declarative-quality-cuda-proof-v1',status='passed',rows=results,seconds=time.monotonic()-began,
        code_sha256=sha(__file__),source=source_binding(selected,library),
        scope='Recorded native fixture inputs, warm versus restored versus restored with observer; no new scientific or normal parent updates.')
    write(output,result);return result


def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--library',type=Path,required=True);args=parser.parse_args()
    try:
        result=verify(args.output,args.library);print(json.dumps(dict(status=result['status'],profiles=len(result['rows']),seconds=result['seconds'])));return 0
    except (ValueError,RuntimeError,OSError,KeyError) as error:
        print(json.dumps(dict(code='quality-study.cuda-proof',severity='error',stage='verify',entity=str(args.output),message=str(error))),file=sys.stderr);return 2


if __name__=='__main__':raise SystemExit(main())
