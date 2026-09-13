"""Bounded policy/config check, indexed cost and optional v1 transition oracle."""
import argparse
from copy import deepcopy
import importlib.util
import json
from pathlib import Path
import statistics
import sys
import time
from unittest.mock import patch

sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from blueprint_ppo_store import read, write
from blueprint_ppo_update import digest, require
from declarative_learning_launch import validate
from declarative_replay_retention import ReplacementIndex


def benchmark(config):
    """One index per update, 256 removals/admissions at production capacity."""
    policies={'v1':deepcopy(config['success']['retention'])}
    policies['v2']=dict(policies['v1'],schema='declarative-replay-category-v2',category_fraction=.1,
        protection_updates=64,immature_order='area_capped_exterior')
    policies['v1']['schema']='declarative-replay-utility-v1'
    strata=config['strata'];names=list(strata)
    entries={str(i):dict(id=str(i),admitted_update=100-i%80,labels=[{}]*(1+i%64),
        input={'draw':{'product':strata[names[i%len(names)]][0]}},
        retention_features_v1=dict(has_place=i%19==0,gain=.01+(i%90)/100,exterior_release=.02),
        utility=None if i%3 else dict(ema=(i%37)/10,draws=8,updates=2,last_update=99)) for i in range(4096)}
    rows=[]
    for name,policy in policies.items():
        times=[]
        for repeat in range(5):
            values=deepcopy(entries);began=time.perf_counter()
            index=ReplacementIndex(values,101,policy,strata)
            for i in range(256):
                incoming=dict(entries[str(i+256)],id=f'new-{i}',admitted_update=101)
                selected,_=index.choose(incoming);del values[selected]
                values[incoming['id']]=incoming;index.add(incoming)
            times.append(time.perf_counter()-began)
            require(len(values)==4096 and sum(len(e['labels']) for e in values.values())<=262144,'retention verify capacity')
        rows.append(dict(policy=name,median_seconds=statistics.median(times),seconds=times,
            entries=4096,removals_per_update=256,index_builds_per_update=1,source_loads=0))
    return rows


def load_module(name,path):
    spec=importlib.util.spec_from_file_location(name,path);module=importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module);return module


def benchmark_sampling(config):
    import torch
    torch.set_num_threads(1)
    from declarative_replay_sampling import Distribution
    sequences=[dict(id=f'{i:04d}',stratum='large' if i%2 else 'small',
        retention_features_v1=dict(has_place=i%19==0,gain=.01+(i%23)/100,exterior_release=.03),
        sampling_priority_v1=dict(value=(i%100)/10,observed_update=1)) for i in range(4096)]
    rows=[]
    for kind in ('uniform','score_mixture','positive_advantage'):
        for count in (8,16,32):
            settings=dict(schema='declarative-success-sampling-v1',kind=kind,sequences_per_minibatch=count)
            if kind=='score_mixture':settings.update(mixture=.5,score_profile='rare_3pp')
            if kind=='positive_advantage':settings.update(alpha=.6,beta=.1,epsilon=1e-6)
            builds,draws=[],[]
            for _ in range(5):
                began=time.perf_counter();distribution=Distribution(sequences,settings,'balanced_available')
                builds.append(time.perf_counter()-began)
                generator=torch.Generator().manual_seed(5_029_003);order=torch.Generator().manual_seed(6_029_003)
                began=time.perf_counter()
                for _ in range(64):distribution.draw(generator,order)
                draws.append(time.perf_counter()-began)
            rows.append(dict(kind=kind,count=count,entries=4096,calls=64,median_build_seconds=statistics.median(builds),
                median_draw_seconds=statistics.median(draws),source_loads=0,forward_calls=0))
    return rows


def transitions(previous):
    """Real native labels, nonempty Adam, replay and eviction across two updates."""
    import torch
    import test_declarative_learning as fixture
    import declarative_learning_policy as policy
    from declarative_learning_buffer import Buffer
    from declarative_learning_runtime import Learner
    native=fixture.NativeLearningTests();native.setUp();chunks=native.preparation_chunks()
    config=deepcopy(native.config)
    from test_declarative_replay_retention import CONFIG
    config['success'].update(retention=CONFIG,max_sequences=1)
    learner=Learner(17,config,'equivalence');evidence=fixture.MemoryEvidence()
    def update(value,payload,source):
        payload=deepcopy(payload)
        with torch.no_grad():lp,ent,values=policy.predict(value.model,policy.prepare_rows(payload['samples']))
        for row,l,e,v in zip(payload['samples'],lp,ent,values,strict=True):
            row.update(old_logp=float(l),old_value=float(v),entropy=float(e))
        payload['model_before']=digest(value.model.state_dict())
        return value.update(payload,source)
    for chunk in chunks:update(learner,chunk,evidence)
    saved=deepcopy(learner.checkpoint([]));entries=saved['buffer']['entries']
    old=entries.pop(next(iter(entries)));old['id']='retired';entries['retired']=old
    old_buffer=load_module('previous_buffer',Path(previous)/'declarative_learning_buffer.py').Buffer
    old_index=load_module('previous_retention',Path(previous)/'declarative_replay_retention.py').ReplacementIndex
    old_kernel=load_module('previous_kernel',Path(previous)/'blueprint_execution_update.py').update_packed
    from blueprint_execution_update import update_packed
    def previous_kernel(*args,**kwargs):
        require(kwargs.pop('replay_sampling',None) is None,'retention verify baseline must use default sampling')
        return old_kernel(*args,**kwargs)
    runs=[]
    for buffer_type,index_type,kernel in ((old_buffer,old_index,previous_kernel),(Buffer,ReplacementIndex,update_packed)):
        restored=Learner(17,config,'equivalence',saved);restored.buffer=buffer_type(saved['buffer'])
        source=fixture.MemoryEvidence();source.chunks=deepcopy(evidence.chunks);records=[]
        with patch('declarative_replay_retention.ReplacementIndex',index_type),patch('declarative_learning_runtime.update_packed',kernel):
            for chunk in chunks:
                record=update(restored,chunk,source)
                records.append(dict(checkpoint=digest(restored.checkpoint([])),events=record['events'],
                    selected_ids=record['ppo']['success']['selected_ids'],replay_rng=record['ppo']['success']['rng_sha256'],
                    order_rng=record['ppo']['success']['order_rng_sha256'],weighted_loss=record['ppo']['success']['weighted_loss']))
        runs.append(records)
    require(runs[0]==runs[1],'retention verify v1 transition mismatch')
    require(any(e['kind']=='evicted' for r in runs[0] for e in r['events']),'retention verify eviction was not exercised')
    return dict(updates=2,nonempty_adam=True,replay_and_eviction=True,exact_checkpoint_events_selected_ids=True,
        baseline_runtime=str(Path(previous).resolve()),records_sha256=digest(runs[0]))


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--config',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--baseline-runtime',type=Path)
    parser.add_argument('--component',choices=('retention','sampling'),default='retention')
    parser.add_argument('--validate-only',action='store_true',help='Validate configuration without repeating measurements')
    args=parser.parse_args()
    try:
        config=read(args.config);validate(config)
        result=dict(schema='declarative-retention-verification-v1',status='passed',config_sha256=digest(config))
        if not args.validate_only:
            result['benchmark']=(benchmark if args.component=='retention' else benchmark_sampling)(config)
            if args.baseline_runtime:result['v1_equivalence']=transitions(args.baseline_runtime)
        write(args.output,result);print(json.dumps(result))
        return 0
    except (ValueError,KeyError,TypeError,OSError) as error:
        print(json.dumps(dict(code='declarative.replay-retention.input',severity='error',stage='verify',
            message=str(error),entity=str(args.config))),file=sys.stderr)
        return 2


if __name__=='__main__':raise SystemExit(main())
