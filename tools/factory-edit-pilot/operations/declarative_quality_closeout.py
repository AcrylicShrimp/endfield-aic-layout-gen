"""Read-only time-aware closeout of the five bound readiness updates."""
import argparse
from collections import defaultdict
from copy import deepcopy
import json
from pathlib import Path
import sys
import time

sys.path.insert(0,str(Path(__file__).resolve().parent))
from declarative_quality_branch import BASE,ROOT,checked,digest,load,parent,read,require,sha,source_binding,write
from declarative_quality_readiness import authenticate
from blueprint_execution_resources import ResourceGuard

CORE=('blueprint_execution_update.py','declarative_learning_runtime.py','declarative_learning_buffer.py',
      'declarative_replay_sampling.py','declarative_success_features.py','declarative_replay_retention.py',
      'operations/declarative_quality_branch.py','operations/declarative_quality_observer.py')


def value(directory):
    directory=Path(directory).resolve();launch=read(directory/'launch.json');proof=read(directory/'quality-branch.json')
    require(sha(directory/'launch.json')==proof['launch_sha256'],'quality-study.closeout: changed launch')
    owner=directory/f'seed-{launch["config"]["seeds"][0]}';job=read(owner/'state.json')
    require(read(directory/'state.json')['status']==job['status']=='complete','quality-study.closeout: incomplete branch')
    saved=load(checked(job['latest_checkpoint'],owner))
    require(saved['protocol_sha256']==sha(directory/'launch.json') and saved['update']==job['update'] and
        saved['model_sha256']==digest(saved['model']),'quality-study.closeout: changed checkpoint')
    return dict(directory=directory,owner=owner,launch=launch,job=job,saved=saved,proof=proof)


def bound(path):return dict(path=str(Path(path).resolve()),sha256=sha(path))


def close(output,execution_source,cuda_proof,receipt,*,seconds=120.):
    output=Path(output).resolve();receipt=Path(receipt).resolve()
    require(not receipt.exists(),'quality-study.closeout: fresh receipt required')
    left,right=value(output/'uninterrupted'),value(output/'restored');first=left['proof']['first_update']
    require(left['proof']['profile']==right['proof']['profile']=='score-8' and
        left['saved']['update']==first+4 and right['saved']['update']==first+2 and
        right['proof']['first_update']==first+1,'quality-study.closeout: exact five-update allocation required')
    require(0<seconds<=120.,'quality-study.closeout: bounded positive allowance required')
    limits=left['launch']['config']['limits'];guard=ResourceGuard(output,seconds,limits['matrix_rss_bytes'],limits['min_free_bytes'])
    for v in (left,right):authenticate(v,execution_source);guard()
    cuda=read(cuda_proof)
    from declarative_quality_branch import PROFILES
    require(cuda['schema']=='declarative-quality-cuda-proof-v1' and cuda['status']=='passed' and
        [r['profile'] for r in cuda['rows']]==list(PROFILES) and all(r['device']=='cuda:0' and r['nonempty_adam'] for r in cuda['rows']),
        'quality-study.closeout: complete identical-input CUDA proof required')
    require(cuda['code_sha256']==sha(Path(__file__).with_name('declarative_quality_cuda_verify.py')),
        'quality-study.closeout: CUDA verifier changed')
    core={}
    for name in CORE:
        key='tools/factory-edit-pilot/'+name;expected=left['launch']['source']['files_sha256'][key]
        require(expected==right['launch']['source']['files_sha256'][key]==cuda['source']['files_sha256'][key]==sha(BASE/name),
            f'quality-study.closeout: execution core changed: {key}')
        core[key]=expected
    live_job=read(output/'uninterrupted/live-job.json')
    before=load(checked(live_job['latest_checkpoint'],left['owner']))
    initial=right['owner']/f'initial/execution-{first+1:04d}.pt.gz';after=load(initial)
    require(before.keys()==after.keys() and all(digest(before[k])==digest(after[k]) for k in before if k!='protocol_sha256'),
        'quality-study.closeout: declared protocol-only state migration differs')
    require(before['protocol_sha256']==sha(left['directory']/'launch.json') and
        after['protocol_sha256']==sha(right['directory']/'launch.json'),'quality-study.closeout: initial protocol binding')
    from declarative_learning_environment import Runtime,Stream
    from declarative_learning_verify_curriculum import compare_restore
    runtime=Runtime(right['launch']['config'],right['launch']['source']['native_library']);snapshots=[]
    for saved in after['streams']:
        guard();require(saved['native'] is not None and saved['expected'] is not None,'quality-study.closeout: missing live semantic boundary')
        stream=Stream.restore(runtime,saved);snapshot=stream.snapshot()
        a,b=deepcopy(saved),deepcopy(snapshot)
        require(a['native']['elapsed_ns']<=b['native']['elapsed_ns'],'quality-study.closeout: restored elapsed allowance reset')
        b['native']['elapsed_ns']=a['native']['elapsed_ns']
        require(digest(a)==digest(b),'quality-study.closeout: native state changed beyond monotonic elapsed')
        snapshots.append(snapshot);del stream
    compare_restore(after['streams'],snapshots)
    rows=[]
    for v,start,end in ((left,first+1,first+4),(right,first+2,first+2)):
        for n in range(start,end+1):
            guard();record=read(checked(v['job']['records'][n-1],v['owner']))
            require(record['update']==n and record['actions']==1024 and not record['full_trace_retained'] and
                record['buffer']['own_entries']==4096,'quality-study.closeout: normal publication/capacity mismatch')
            chunk=load(checked(v['job']['replay_sources'][str(n)],v['owner']))
            if n%2==0:require(len(chunk['completed'])==32 and all(r['result']['factory_validated'] for r in chunk['completed']),
                'quality-study.closeout: invalid completed native outputs')
            rows.append(dict(branch=v['directory'].name,update=n,record=bound(v['owner']/v['job']['records'][n-1]['path']),
                collection_seconds=record['collection_seconds'],learner_seconds=record['ppo']['execution']['update_seconds'],
                publication_seconds=record['publication_seconds'],evictions=sum(e['kind']=='evicted' for e in record['events']),
                completed=len(chunk['completed']),native_validated=True,compact_publication=True))
    require(sum(r['evictions'] for r in rows)>0,'quality-study.closeout: eviction not exercised')
    from declarative_replay_storage import references
    coverage=[]
    for v,states in ((left,[left['saved'],before]),(right,[right['saved'],after])):
        needed=defaultdict(set)
        for state in states:
            for n,indices in references(state['buffer']).items():needed[n].update(indices)
        for n,indices in sorted(needed.items()):
            guard();chunk=load(checked(v['job']['replay_sources'][str(n)],v['owner']))
            require(indices<=chunk['samples'].keys(),'quality-study.closeout: required live/replay row missing')
        coverage.append(dict(branch=v['directory'].name,sources=len(needed),rows=sum(map(len,needed.values()))))
    main=parent(left['proof']['parent']['directory'])
    require(main['binding']==left['proof']['parent'],'quality-study.closeout: main parent changed')
    source=source_binding(main['launch']['config'],main['launch']['source']['native_library'])
    result=dict(schema='declarative-quality-readiness-v1',status='passed',continuity_closed=True,parent=main['binding'],
        source=source,execution_source=dict(directory=str(Path(execution_source).resolve()),source=left['launch']['source']),
        execution_core_sha256=core,cuda_proof=bound(cuda_proof),exact_initial_checkpoint=True,
        live_streams_restored=len(snapshots),native_elapsed_monotonic=True,unchanged_native_limits=True,
        future_trajectory_equality_required=False,executed_updates=5,rows=rows,required_sources=coverage,
        initial_metadata=bound(output/'initial-metadata.json'),closeout_seconds=time.monotonic()-guard.began,
        initial_checkpoint=bound(checked(live_job['latest_checkpoint'],left['owner'])),restored_initial_checkpoint=bound(initial),
        original_failure=bound(output/'failure.json') if (output/'failure.json').exists() else None,
        scope='Exact saved-state restore and valid live followup; exact CUDA transitions use identical recorded fixture inputs.')
    guard();write(receipt,result);return result


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    for name in ('output','execution-source','cuda-proof','receipt'):parser.add_argument('--'+name,type=Path,required=True)
    args=parser.parse_args()
    try:
        result=close(args.output,args.execution_source,args.cuda_proof,args.receipt)
        print(json.dumps({k:result[k] for k in ('status','executed_updates','closeout_seconds','live_streams_restored')}));return 0
    except (ValueError,RuntimeError,OSError,KeyError) as error:
        print(json.dumps(dict(code='quality-study.closeout',severity='error',stage='verify',entity=str(args.output),message=str(error))),file=sys.stderr);return 2


if __name__=='__main__':raise SystemExit(main())
