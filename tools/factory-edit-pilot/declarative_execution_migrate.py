"""Explicit paused-run source migration with compact, immutable replay sources."""
from collections import defaultdict
from copy import deepcopy
from pathlib import Path
import os
import time

from blueprint_execution_resources import ResourceGuard,cpu_allocation
from blueprint_ppo_store import checked,load,read,reference,save,sha,write
from blueprint_ppo_update import digest,require
from declarative_learning_launch import sources,validate
from declarative_learning_runtime import CHECKPOINT,account_write
from declarative_replay_storage import compact,references


def parent(directory):
    directory=Path(directory).resolve();launch=read(directory/'launch.json');matrix=read(directory/'state.json')
    require(matrix['status']=='paused' and launch['config'].get('duration'), 'declarative.execution-migration: paused duration run required')
    seed=launch['config']['seeds'][0];owner=directory/f'seed-{seed}';job=read(owner/'state.json')
    require(job['status']=='paused' and job['stop_reason']=='requested_stop', 'declarative.execution-migration: durable requested pause required')
    saved=load(checked(job['latest_checkpoint'],owner))
    require(saved['schema']==CHECKPOINT and saved['protocol_sha256']==sha(directory/'launch.json') and
        saved['update']==job['update'] and saved['model_sha256']==digest(saved['model']), 'declarative.execution-migration: parent checkpoint identity')
    require(len(saved['streams'])==launch['config']['training']['streams'] and all(s['native'] is None and s['next_start'] is not None for s in saved['streams']),
        'declarative.execution-migration: selected complete-wave boundary required')
    binding=dict(directory=str(directory),launch_sha256=sha(directory/'launch.json'),state_sha256=sha(directory/'state.json'),
        job_sha256=sha(owner/'state.json'),checkpoint=job['latest_checkpoint'])
    return dict(directory=directory,owner=owner,launch=launch,matrix=matrix,job=job,saved=saved,binding=binding)


def compact_parent(directory, output):
    """One-off migration; reuse its receipt instead of re-decoding the parent."""
    value=parent(directory);output=Path(output).resolve()
    require(not output.exists(),f'declarative.execution-migration: fresh compact-source output required: {output}')
    output.mkdir(parents=True);began=time.monotonic();needed=defaultdict(set);checkpoints={}
    limits=value['launch']['config']['limits']
    guard=ResourceGuard(output,900,limits['matrix_rss_bytes'],limits['min_free_bytes'])
    for name in value['job']['retained_files']:
        guard()
        path=Path(name)
        if path.parent.name not in ('checkpoints','initial') or not path.name.endswith('.pt.gz'):continue
        saved=load(path)
        for ordinal,indices in references(saved['buffer']).items():needed[ordinal].update(indices)
        checkpoints[str(saved['update'])]=sorted(references(saved['buffer']))
    # latest_checkpoint may have been produced by an earlier explicit migration.
    for ordinal,indices in references(value['saved']['buffer']).items():needed[ordinal].update(indices)
    refs={};rows=[];checked_roundtrip=False
    for ordinal,indices in sorted(needed.items()):
        guard()
        job=value['job'];owner=value['owner']
        source_ref=job['replay_sources'][str(ordinal)] if 'replay_sources' in job else read(checked(job['records'][ordinal-1],owner))['chunk']
        path=checked(source_ref,owner);payload=load(path);retained=compact(payload,indices)
        target=output/f'replay/{ordinal:04d}.pt.gz';save(target,retained);ref=reference(target,output);refs[str(ordinal)]=ref
        # One actual roundtrip complements the serializer and cross-boundary
        # tests. Do not replay or rehash every tensor in every source again.
        if not checked_roundtrip and retained['samples']:
            require(digest(retained)==digest(load(target)), 'declarative.execution-migration: compact roundtrip differs')
            checked_roundtrip=True
        rows.append(dict(update=ordinal,samples=len(indices),original_samples=retained['sample_count'],
            original_bytes=path.stat().st_size,compact_bytes=target.stat().st_size,source=source_ref))
        del payload,retained
        print(dict(stage='compact-replay',completed=len(rows),sources=len(needed)),flush=True)
    result=dict(schema='declarative-compact-parent-v1',status='passed',parent=value['binding'],replay_sources=refs,
        checkpoint_chunks=checkpoints,references_sha256=digest({k:sorted(v) for k,v in needed.items()}),
        rows=rows,seconds=time.monotonic()-began,roundtrip_checked=checked_roundtrip)
    write(output/'manifest.json',result);return result


def compatible(config, previous):
    for key in ('schema','chunk_schema','seeds','device','actions','native','training','ppo','success',
                'strata','evaluation','panel','duration','limits','start_curriculum','execution'):
        require(config[key]==previous[key],f'declarative.execution-migration: changed learning or duration contract {key}')
    for key in ('schema','kind','spec','seed_base','max_attempts','episode_setup_seconds','canonical_streams'):
        require(config['training_input'][key]==previous['training_input'][key],f'declarative.execution-migration: changed generated input contract {key}')
    require(config['retention']['checkpoints']==previous['retention']['checkpoints'] and
        config['retention']['evaluation_traces']==previous['retention']['evaluation_traces'] and
        config['retention']['recovery_updates']==previous['retention']['recovery_updates'], 'declarative.execution-migration: changed retained checkpoints')


def relocate_inputs(source, output, config):
    """Link actual inputs unchanged; explicitly rebind only the catalog locator."""
    from blueprint_ppo_store import ROOT
    from random_factory_generation import resolve_spec
    source=Path(source).resolve();output=Path(output).resolve()
    require(source!=output and not output.exists(), 'declarative.execution-migration: fresh independent input store required')
    manifest=read(source/'manifest.json');catalogs=read(source/'catalogs.json')
    selected=resolve_spec(ROOT/config['training_input']['spec'])['catalogs']
    require(set(catalogs)==set(selected) and all(sha(catalogs[k])==sha(selected[k]) for k in selected),
        'declarative.execution-migration: relocated catalogs differ in content')
    require(manifest['seed_base']==config['training_input']['seed_base'] and
        manifest['spec_sha256']==sha(ROOT/config['training_input']['spec']), 'declarative.execution-migration: changed input store contract')
    output.mkdir(parents=True);linked=0
    # This is an explicit one-off migration, never a training-loop directory
    # scan. Per-input integrity remains checked by the unchanged input loader.
    for path in source.rglob('*'):
        require(not path.is_symlink(),f'declarative.execution-migration: input store symlink {path}')
        if not path.is_file() or path.name.endswith('.lock'):continue
        target=output/path.relative_to(source);target.parent.mkdir(parents=True,exist_ok=True);os.link(path,target);linked+=1
    # Atomic replacement detaches the locator's link before changing it. Actual
    # episode, generation, request and layout files keep their original hashes.
    write(output/'catalogs.json',selected)
    receipt=dict(schema='declarative-input-location-migration-v1',source=str(source),output=str(output),
        source_manifest_sha256=sha(source/'manifest.json'),linked_files=linked,
        source_catalogs=catalogs,catalogs=selected,catalogs_sha256=sha(output/'catalogs.json'))
    write(output/'location-migration.json',receipt);return receipt


def migrate(config, library, panel, directory, compact_directory, verification, output):
    value=parent(directory);validate(config);compatible(config,value['launch']['config'])
    output=Path(output).resolve();compact_directory=Path(compact_directory).resolve()
    require(not output.exists(),f'declarative.execution-migration: fresh run output required: {output}')
    compacted=read(compact_directory/'manifest.json');proof=read(verification);source=sources(config,library)
    require(compacted['status']=='passed' and compacted['parent']==value['binding'], 'declarative.execution-migration: stale compact-source receipt')
    require(proof['status']=='passed' and proof['continuity_closed'] and proof['parent']==value['binding'] and
        proof['source_sha256']==source['code_sha256'] and proof['config_sha256']==digest(config) and
        proof['compact_manifest_sha256']==sha(compact_directory/'manifest.json'), 'declarative.execution-migration: matching completed execution verification required')
    require(source['native_sha256']==value['launch']['source']['native_sha256'] and sha(panel)==value['launch']['panel']['sha256'],
        'declarative.execution-migration: native binary or fixed evaluation changed')
    allocation=cpu_allocation(config.get('execution_options',{}).get('workers',8))
    require(allocation['workers']==proof['allocation']['workers'], 'declarative.execution-migration: verified worker allocation changed')
    return initialize(value,config,source,compacted,compact_directory,verification,output,allocation)


def initialize(value, config, source, compacted, compact_directory, verification, output, allocation, *, diagnostic=False):
    selected=deepcopy(config)
    # The private append-only bank preserves all actual fresh inputs, including
    # retained unfinished recovery checkpoints, without modifying its parent.
    input_source=selected['training_input']['directory'] if diagnostic else value['launch']['config']['training_input']['directory']
    input_receipt=relocate_inputs(input_source,output/'training-inputs',selected)
    selected['training_input']['directory']=input_receipt['output']
    launch=deepcopy(value['launch']);launch.update(config=selected,source=source,allocation=allocation,
        execution_migration=dict(parent=value['binding'],input_location=input_receipt,
            compact_source=dict(directory=str(compact_directory),manifest_sha256=sha(compact_directory/'manifest.json'))),
        verification=dict(path=str(Path(verification).resolve()),sha256=sha(verification)),
        duration_ready=not diagnostic,created_unix_seconds=time.time())
    write(output/'launch.json',launch)
    owner=output/f'seed-{selected["seeds"][0]}';owner.mkdir();job=deepcopy(value['job']);saved=deepcopy(value['saved'])
    job.update(status='ready',retained_files={},retained_bytes=0,replay_sources=compacted['replay_sources'],checkpoint_chunks=compacted['checkpoint_chunks'])
    # Original full trajectories stay in the immutable predecessor. Compact
    # source records and their origin references preserve selected replay here.
    for name in value['job']['retained_files']:
        path=Path(name)
        if path.parent.name in ('chunks','replay'):continue
        target=owner/path.relative_to(value['owner']);target.parent.mkdir(parents=True,exist_ok=True)
        os.link(path,target);account_write(job,target)
    for ref in compacted['replay_sources'].values():
        path=checked(ref,compact_directory);target=owner/ref['path'];target.parent.mkdir(parents=True,exist_ok=True)
        os.link(path,target);account_write(job,target)
    if diagnostic:
        first=saved['update'];job.update(seconds=0.,evaluation=[r for r in job['evaluation'] if r['update']==first],
            pause_boundaries=[first],resource={},wandb_id=None,wandb_url=None)
    preserved={k:digest(saved[k]) for k in ('model','actor_optimizer','critic_optimizer','sampler_rng','shuffle_rng','buffer','streams')}
    saved['protocol_sha256']=sha(output/'launch.json')
    target=owner/f'initial/execution-{saved["update"]:04d}.pt.gz';save(target,saved);account_write(job,target)
    job['latest_checkpoint']=reference(target,owner)
    job.setdefault('execution_migrations',[]).append(dict(parent=value['binding'],update=saved['update'],seconds=job['seconds']))
    write(owner/'state.json',job)
    state=deepcopy(value['matrix']);state.update(status='ready',launch_sha256=sha(output/'launch.json'),jobs={})
    if diagnostic:state['seconds']=0.
    write(output/'state.json',state)
    receipt=dict(schema='declarative-execution-migration-v1',status='ready',parent=value['binding'],preserved_sha256=preserved,
        checkpoint=job['latest_checkpoint'],completed_update=job['update'],preserved_seconds=job['seconds'],
        remaining_seconds=config['duration']['seconds']-job['seconds'],input_location=input_receipt)
    write(output/'execution-migration.json',receipt);return receipt


def diagnostic(config, library, directory, compact_directory, inputs, output):
    """Prepare exactly one isolated normal-command wave, with online tracking off."""
    value=parent(directory);compact_directory=Path(compact_directory).resolve();output=Path(output).resolve()
    require(not output.exists(),'declarative.execution-diagnostic: fresh output required')
    compacted=read(compact_directory/'manifest.json')
    require(compacted['parent']==value['binding'] and compacted['status']=='passed','declarative.execution-diagnostic: stale compact sources')
    selected=deepcopy(config);first=value['saved']['update']
    selected['training_input']['directory']=str(Path(inputs).resolve())
    selected['training'].update(first_update=first,updates=first+2)
    selected['evaluation']['at']=[first]
    selected['duration'].update(seconds=.001,finish_grace_seconds=450.)
    selected['limits'].update(arm_seconds=450.001,matrix_seconds=480.)
    selected['retention']['checkpoints']=sorted({*selected['retention']['checkpoints'],first,first+1,first+2})
    selected['tracking']['mode']='disabled'
    validate(selected);allocation=cpu_allocation(selected['execution_options']['workers'])
    return initialize(value,selected,sources(selected,library),compacted,compact_directory,compact_directory/'manifest.json',output,allocation,diagnostic=True)
