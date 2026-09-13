"""Inspect survivor coverage or capture a bounded complete terminal-arrival corpus."""
import argparse
from collections import Counter, defaultdict
from copy import deepcopy
import json
import os
from pathlib import Path
import sys
import time
from types import SimpleNamespace
from unittest.mock import patch

sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from blueprint_execution_resources import ResourceGuard
from blueprint_ppo_store import checked, load, read, reference, save, sha, write
from blueprint_ppo_update import digest, require
from declarative_execution_migrate import parent
from declarative_replay_storage import compact, references
from declarative_success_features import extract

SCHEMA='declarative-success-arrivals-v1'


def row_sets(value):
    return {int(k):sorted(v) for k,v in value.items() if v}


def entry_references(entries):
    return references(dict(entries={str(i):e for i,e in enumerate(entries)},active={}))


def inspect_parent(value, output, updates=128):
    output=Path(output);require(not output.exists(),'success-arrivals.output: fresh inspect output required')
    latest=value['saved']['update'];entries=value['saved']['buffer']['entries'];counts=Counter();missing=[]
    records=[]
    for n in range(max(1,latest-updates+1),latest+1):
        ref=value['job']['records'][n-1];record=read(checked(ref,value['owner']))
        require(record['update']==n,'success-arrivals.order: historical update gap')
        records.append(ref)
        for event in record['events']:
            counts[event['kind']]+=1
            if event['kind'] in ('admitted','duplicate'):
                entry=entries.get(event['id'])
                # A surviving ID cannot bind an earlier duplicate/readmission's
                # actual references. Ordinary update events do not retain those.
                if event['kind']=='duplicate' or entry is None or entry['admitted_update']!=n:
                    missing.append(dict(update=n,id=event['id'],kind=event['kind'],
                        reason='terminal_attempt_reference_binding_not_retained'))
    result=dict(schema=SCHEMA,status='incomplete_history' if missing else 'unproven_history',
        complete=False,parent=value['binding'],interval=[max(1,latest-updates+1),latest],events=dict(counts),
        missing_attempt_bindings=missing,records=records,
        conclusion='A survivor pool and update events do not establish all ordered terminal attempts; collect an isolated interval.')
    output.mkdir(parents=True);write(output/'report.json',result)
    return result


class Collector:
    """Diagnostic-only union; normal replay roots and checkpoints stay untouched."""
    def __init__(self, value, output, guard, max_bytes=2*1024**3):
        self.value,self.output,self.guard=value,Path(output),guard
        self.max_bytes,self.added_bytes= max_bytes,0
        self.union,self.baseline_sources,self.boundaries={},{},[]
        self.checkpoint_roots={};self.pending=[];self.current=None
        self.initial_samples={};self.initial_pool=[];self.write_seconds=0.
        self.existing_files=set();self.input_folders=set();self.new_input_bytes=0;self.peak_added_bytes=0
        self.first=value['saved']['update'];self.counters=Counter()
        self.output.mkdir(parents=True)

    def write_object(self,path,value):
        began=time.perf_counter();save(path,value)
        self.write_seconds+=time.perf_counter()-began
        self.added_bytes+=path.stat().st_size
        require(self.added_bytes<=self.max_bytes,'success-arrivals.output-limit: diagnostic evidence byte cap')
        return reference(path,self.output)

    def source(self,ordinal,payload,path,baseline_ref=None):
        require(all(isinstance(i,int) for i in payload['samples']),'success-arrivals.source: compact row map required')
        row=dict(update=ordinal,reference=reference(path,self.output),bytes=path.stat().st_size,
            coverage=sorted(payload['samples']),sample_count=payload['sample_count'])
        if baseline_ref is not None:row['baseline_reference']=baseline_ref
        self.union[ordinal]=row

    def initialize(self):
        value=self.value;needed=defaultdict(set)
        for name in value['job']['retained_files']:
            path=Path(name)
            if path.parent.name not in ('checkpoints','initial') or not path.name.endswith('.pt.gz'):continue
            self.guard()
            state=value['saved'] if path==value['owner']/value['job']['latest_checkpoint']['path'] else load(path)
            refs=row_sets(references(state['buffer']))
            # The explicitly rebound latest checkpoint takes precedence over an
            # older same-update migration copy; its physical roots are identical.
            self.checkpoint_roots[state['update']]=dict(checkpoint=reference(path,value['owner']),rows=refs)
            for n,indices in refs.items():needed[n].update(indices)
        current=row_sets(references(value['saved']['buffer']))
        self.checkpoint_roots[self.first]=dict(checkpoint=value['job']['latest_checkpoint'],rows=current)
        for n,indices in current.items():needed[n].update(indices)
        for n in sorted(int(k) for k in value['job']['replay_sources']):
            indices=needed.get(n,set())
            self.guard();ref=value['job']['replay_sources'][str(n)];path=checked(ref,value['owner']);payload=load(path)
            require(isinstance(payload['samples'],dict),'success-arrivals.source: expected existing compact source')
            require(set(indices)<=payload['samples'].keys(),f'success-arrivals.missing-row: initial source {n}')
            target=self.output/f'union/{n:04d}.pt.gz';target.parent.mkdir(parents=True,exist_ok=True);os.link(path,target)
            self.source(n,payload,target,ref)
            self.baseline_sources[n]=dict(reference=ref,bytes=path.stat().st_size,coverage=sorted(payload['samples']))
            for i in indices:self.initial_samples[n,i]=payload['samples'][i]
        for entry in value['saved']['buffer']['entries'].values():
            item=deepcopy(entry)
            item['retention_features_v1']=extract([self.initial_samples[tuple(l['reference'])] for l in entry['labels']],entry['id'])
            self.initial_pool.append(item)
        self.initial_ref=self.write_object(self.output/'initial.pt.gz',dict(entries=self.initial_pool,
            checkpoint_roots=self.checkpoint_roots))
        checkpoint=self.output/'parent-checkpoint.pt.gz'
        os.link(checked(value['job']['latest_checkpoint'],value['owner']),checkpoint)
        self.parent_checkpoint=reference(checkpoint,self.output)

    def warm(self,learner):
        learner.buffer.hydrate(SimpleNamespace(sample=lambda ref:self.initial_samples[tuple(ref)]),learner.config['strata'])
        self.initial_samples.clear()

    def observe(self,key,candidate,evidence):
        # Compact metadata only. Rows are extracted together at publication while
        # the current payload and previous live-credit source remain available.
        self.pending.append(dict(key=key,entry=deepcopy(candidate)))

    def before_publish(self,learner,payload,record,evidence):
        self.guard();ordinal=learner.ordinal;needed=set(references(learner.buffer.state).get(ordinal,set()))
        bank=Path(learner.config['training_input'].get('directory','.')).resolve()
        for before in payload['native_before']:
            metadata=before['metadata']['training_input']
            if before['policy_actions'] or metadata is None:continue
            root=Path(metadata.get('input_directory',bank)).resolve()
            if root!=bank:continue
            folder=root/Path(metadata['reference']['path']).parent
            if folder in self.input_folders:continue
            self.input_folders.add(folder)
            # Visit only this newly consumed slot, once. Existing linked files
            # were already present in the immutable input bank before capture.
            for path in folder.rglob('*'):
                if path.is_file():
                    stat=path.stat()
                    if stat.st_nlink==1:self.new_input_bytes+=stat.st_size
        for attempt in self.pending:
            entry=attempt['entry'];samples=[evidence.sample(l['reference']) for l in entry['labels']]
            require(all('input' in s and 'decision' in s for s in samples),'success-arrivals.policy-rows: missing packed input')
            entry['retention_features_v1']=extract(samples,entry['id'])
            needed.update(l['reference'][1] for l in entry['labels'] if l['reference'][0]==ordinal)
        retained=compact(payload,needed);path=self.output/f'union/{ordinal:04d}.pt.gz'
        self.write_object(path,retained);self.source(ordinal,retained,path)
        for attempt in self.pending:
            for label in attempt['entry']['labels']:
                n,i=label['reference']
                require(n in self.union and i in self.union[n]['coverage'],f'success-arrivals.missing-row: attempt {attempt["key"]} {n}:{i}')
        self.current=dict(update=ordinal,attempts=self.pending,events=deepcopy(record['events']),
            sequence_utility=record['ppo']['success'].get('sequence_utility',[]),
            selected_ids=record['ppo']['success']['selected_ids'],sil_calls=len(record['ppo']['success']['selected_ids']),
            actor_epochs=record['ppo']['actor_epochs'],epochs=record['ppo']['epochs'],
            model_before=record['ppo']['model_before'],model_after=record['ppo']['model_after'],
            live_roots=row_sets(references(dict(entries={},active=learner.buffer.state['active']))),
            checkpoint_rows=row_sets(references(learner.buffer.state)),
            baseline_buffer=record['buffer'],actions=record['actions'])
        self.pending=[]

    def after_retain(self,learner,directory,job):
        row=self.current;require(row is not None and row['update']==learner.ordinal,'success-arrivals.order: publication gap')
        n=learner.ordinal;self.checkpoint_roots[n]=dict(checkpoint=job['latest_checkpoint'],rows=row['checkpoint_rows'])
        ref=job['replay_sources'][str(n)];path=directory/ref['path']
        self.baseline_sources[n]=dict(reference=ref,bytes=path.stat().st_size,
            coverage=sorted(references(learner.buffer.state).get(n,set())))
        config=learner.config['retention'];keep=set(config['checkpoints'])
        keep.update(e['update'] for e in job.get('best_evaluations',{}).values())
        keep.update(range(max(0,n-config['recovery_updates']+1),n+1))
        row.update(checkpoint=job['latest_checkpoint'],mandatory_checkpoints=sorted(keep & self.checkpoint_roots.keys()),
            baseline_retained_sources=sorted(int(k) for k in job['replay_sources']),
            baseline_retained_bytes=job['retained_bytes'],baseline_update_record=job['records'][-1])
        ordered=[e for e in row['events'] if e['kind'] in ('admitted','duplicate')]
        require([e['id'] for e in ordered]==[a['entry']['id'] for a in row['attempts']],
            'success-arrivals.coverage: ordered attempts differ from baseline events')
        require(len({tuple(a['key']) for a in row['attempts']})==len(row['attempts']),
            'success-arrivals.attempt-key: duplicate key')
        counts=Counter(e['kind'] for e in row['events']);counts['attempts']=len(row['attempts']);self.counters.update(counts)
        row['counts']=dict(counts)
        self.boundaries.append(dict(update=n,reference=self.write_object(self.output/f'boundaries/{n:04d}.pt.gz',row)))
        self.current=None
        added=self.added_bytes+self.new_input_bytes+sum(size for name,size in job['retained_files'].items() if name not in self.existing_files)
        # Small state/manifests are not in the normal artifact ledger. Include
        # their current state size plus a conservative MiB for control receipts.
        if (directory/'state.json').exists():added+=(directory/'state.json').stat().st_size
        added+=1024**2
        self.peak_added_bytes=max(self.peak_added_bytes,added)
        require(added<=self.max_bytes,'success-arrivals.output-limit: total added evidence byte cap')
        write(self.output/'progress.json',dict(update=n,complete_updates=len(self.boundaries),counts=dict(self.counters),
            added_bytes=added,compact_write_seconds=self.write_seconds))

    def finalize(self,child,status,error=None):
        job=read(child/f'seed-{self.value["saved"]["seed"]}/state.json')
        complete=bool(self.boundaries) and self.boundaries[-1]['update']==job['update'] and job['update']%2==0 and status=='complete' and time.monotonic()-self.guard.began<=self.guard.seconds
        report=dict(schema=SCHEMA,status='complete' if complete else 'partial',complete=complete,
            parent=self.value['binding'],parent_checkpoint=self.parent_checkpoint,
            initial_pool=self.initial_ref,initial_entries=len(self.initial_pool),initial_update=self.first,
            final_update=job['update'],boundaries=self.boundaries,counts=dict(self.counters),
            union_sources=self.union,baseline_sources=self.baseline_sources,
            child=dict(directory=str(child),launch_sha256=sha(child/'launch.json'),checkpoint=job['latest_checkpoint']),
            diagnostic_source_sha256=sha(__file__),stop_reason=job.get('stop_reason'),error=error,
            added_diagnostic_bytes=self.added_bytes,compact_write_seconds=self.write_seconds,
            peak_total_added_bytes=self.peak_added_bytes,new_actual_input_bytes=self.new_input_bytes,
            limits=dict(updates=128,seconds=self.guard.seconds,added_diagnostic_bytes=self.max_bytes),
            seconds=time.monotonic()-self.guard.began,resource=self.guard.last,
            scope='Actual baseline arrivals; fixed-arrival comparisons do not predict candidate-generated trajectories.')
        write(self.output/'manifest.json',report);return report


def capture(value,output,environment,updates=128,seconds=1800):
    from declarative_experiment_setup import prepare_baseline
    import declarative_learning_run as runner
    from declarative_learning_runtime import Learner
    output=Path(output).resolve();require(not output.exists(),'success-arrivals.output: fresh capture output required')
    limits=value['launch']['config']['limits']
    guard=ResourceGuard(output,seconds,limits['matrix_rss_bytes'],limits['min_free_bytes'])
    setup=prepare_baseline(value,output,updates=updates,seconds=seconds)
    collector=Collector(value,output/'corpus',guard);collector.initialize()
    initial_job=read(setup['directory']/f'seed-{value["saved"]["seed"]}/state.json')
    collector.existing_files={name for name in initial_job['retained_files'] if Path(name).stat().st_nlink>1}
    class ObservedLearner(Learner):
        def __init__(self,*args,**kwargs):
            super().__init__(*args,**kwargs);collector.warm(self);self.buffer.admission_observer=collector.observe
    original_publish,original_retain=runner.publish,runner.retain
    def publish(learner,payload,record,streams,directory,job,**kwargs):
        collector.before_publish(learner,payload,record,kwargs['evidence'])
        return original_publish(learner,payload,record,streams,directory,job,**kwargs)
    def retain(learner,directory,job):
        original_retain(learner,directory,job);collector.after_retain(learner,directory,job)
        guard()
    class BoundedGuard(ResourceGuard):
        def __call__(self):
            super().__call__();guard()
    error=None;status='partial'
    try:
        with patch.object(runner,'Learner',ObservedLearner),patch.object(runner,'publish',publish),patch.object(runner,'retain',retain),patch.object(runner,'ResourceGuard',BoundedGuard):
            state=runner.run(setup['directory'],environment)
            status=state['status']
    except (ValueError,RuntimeError,TimeoutError,OSError) as failure:
        error=str(failure)
    finally:
        report=collector.finalize(setup['directory'],status,error)
    require(read(value['directory']/'state.json')['status']=='paused' and
        all(sha(path)==expected for path,expected in (
            (value['directory']/'launch.json',value['binding']['launch_sha256']),
            (value['directory']/'state.json',value['binding']['state_sha256']),
            (value['owner']/'state.json',value['binding']['job_sha256']))),
        'success-arrivals.parent: immutable paused parent changed')
    return report


def main():
    parser=argparse.ArgumentParser(description=__doc__);commands=parser.add_subparsers(dest='command',required=True)
    for command in ('inspect','capture'):
        sub=commands.add_parser(command);sub.add_argument('--parent',type=Path,required=True);sub.add_argument('--output',type=Path,required=True)
        if command=='capture':
            sub.add_argument('--environment',type=Path,required=True);sub.add_argument('--updates',type=int,default=128)
            sub.add_argument('--seconds',type=float,default=1800)
    args=parser.parse_args()
    try:
        value=parent(args.parent)
        result=inspect_parent(value,args.output) if args.command=='inspect' else capture(value,args.output,args.environment,args.updates,args.seconds)
        print(json.dumps({k:v for k,v in result.items() if k in ('status','complete','initial_update','final_update','counts','seconds','stop_reason','error')}))
        return 0 if result.get('complete') or args.command=='inspect' else 2
    except (ValueError,KeyError,TypeError,OSError) as error:
        diagnostic=dict(code='success-arrivals.input',severity='error',stage=args.command,message=str(error),entity=str(args.parent))
        if args.output.exists():write(args.output/'failure.json',dict(status='blocked',complete=False,diagnostic=diagnostic))
        print(json.dumps(diagnostic),file=sys.stderr)
        return 2


if __name__=='__main__':raise SystemExit(main())
