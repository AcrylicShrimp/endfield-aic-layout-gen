"""Explicitly import a compatible completed v1 episode boundary into a v2 launch."""
from copy import deepcopy
from pathlib import Path
import os

from blueprint_ppo_store import checked, load, read, reference, save, sha, write
from blueprint_ppo_update import digest, require
from declarative_learning_curriculum import next_plan
from declarative_learning_runtime import account_write, CHECKPOINT


def parent(source, config):
    source=Path(source).resolve();launch=read(source/'launch.json')
    require(len(config['seeds'])==1, 'declarative.continuation: one explicitly selected parent seed required')
    seed=config['seeds'][0];directory=source/f'seed-{seed}';job=read(directory/'state.json')
    ordinal=config['training']['first_update']
    require(job['update']==ordinal, 'declarative.continuation: parent must be at the registered update')
    saved=load(checked(job['latest_checkpoint'],directory))
    require(saved['schema']=='declarative-learning-checkpoint-v1' and saved['update']==ordinal and
        saved['seed']==seed and saved['protocol_sha256']==sha(source/'launch.json'),
        'declarative.continuation: expected source-bound v1 checkpoint')
    require(all(s['native'] is None for s in saved['streams']) and not saved['buffer']['active'],
        'declarative.continuation: import only a completed episode boundary')
    old=launch['config']
    require('start_curriculum' not in old, 'declarative.continuation: this explicit migration accepts fresh-only v1')
    for field in ('ppo','actions','native','strata','device','success'):
        require(config[field]==old[field], f'declarative.continuation: changed retained setting {field}')
    for field in ('streams','chunk_actions','episode_actions','elapsed_ms'):
        require(config['training'][field]==old['training'][field], f'declarative.continuation: changed horizon {field}')
    for field in ('replicates','max_actions','elapsed_ms','action_milestones'):
        require(config['evaluation'][field]==old['evaluation'][field], f'declarative.continuation: changed evaluation {field}')
    for field in ('schema','kind','spec','seed_base','max_attempts','episode_setup_seconds'):
        require(config['training_input'][field]==old['training_input'][field], f'declarative.continuation: changed generator {field}')
    return dict(source=source,directory=directory,launch=launch,job=job,saved=saved,
        binding=dict(directory=str(source),launch=reference(source/'launch.json',source),
            checkpoint=dict(directory=str(directory),**job['latest_checkpoint']),seed=seed,update=ordinal))


def link(source, target, job):
    require(source.is_file(), f'declarative.continuation: missing source evidence {source}')
    target.parent.mkdir(parents=True,exist_ok=True)
    os.link(source,target)
    account_write(job,target)


def initialize(value, launch, output):
    old=value['job'];saved=deepcopy(value['saved']);source=value['directory']
    seed=saved['seed'];ordinal=saved['update'];directory=output/f'seed-{seed}'
    directory.mkdir()
    job=dict(schema='declarative-learning-state-v2',status='ready',update=ordinal,seconds=0.,
        records=[],inputs=deepcopy(old['inputs']),source=value['binding'],first_update=ordinal)
    needed={n for entry in saved['buffer']['entries'].values() for n in entry['episode_chunks']}
    needed.add(ordinal)
    for ref in old['records'][:ordinal]:
        record=read(checked(ref,source))
        link(source/ref['path'],directory/ref['path'],job)
        job['records'].append(ref)
        if record['update'] in needed:
            link(source/record['chunk']['path'],directory/record['chunk']['path'],job)
        if (source/record['checkpoint']['path']).exists():
            link(source/record['checkpoint']['path'],directory/record['checkpoint']['path'],job)
    # Reuse the identical retained evaluation and random baseline; no new rollout
    # is needed to re-evaluate unchanged parent weights on the same frozen panel.
    initial=next(ref for ref in old['evaluation'] if ref['update']==ordinal)
    for key,refs in [('evaluation',[initial]),('random',old['random'])]:
        job[key]=deepcopy(refs)
        for ref in refs:
            checked(ref,source);link(source/ref['path'],directory/ref['path'],job)
    record=read(checked(old['records'][ordinal-1],source))
    payload=load(checked(record['chunk'],source))
    plan,ref=next_plan(launch['config'],seed,directory,payload,record['chunk'],legacy=True)
    job['curriculum_plans']={str(plan['wave']):ref}
    account_write(job,directory/ref['path'])
    for slot in plan['slots']:
        if slot['depth']:account_write(job,directory/slot['input']['path'])
    for stream,slot in zip(saved['streams'],plan['slots'],strict=True):stream['next_start']=slot
    saved.update(schema=CHECKPOINT,protocol_sha256=sha(output/'launch.json'),continuation_source=value['binding'])
    target=directory/f'initial/continued-{ordinal:04d}.pt.gz';save(target,saved);account_write(job,target)
    job['latest_checkpoint']=reference(target,directory)
    job['checkpoint_chunks']={str(ordinal):sorted(needed)}
    write(directory/'state.json',job)
    return dict(seed=seed,update=ordinal,model_sha256=saved['model_sha256'],
        actor_optimizer_sha256=digest(saved['actor_optimizer']),critic_optimizer_sha256=digest(saved['critic_optimizer']),
        sampler_rng_sha256=digest(saved['sampler_rng']),shuffle_rng_sha256=digest(saved['shuffle_rng']),
        replay_entries=len(saved['buffer']['entries']),source_chunks=len(needed),next_plan=ref,
        reuse='Immutable local source records/chunks are hard-linked; parent files are never overwritten or removed.')
