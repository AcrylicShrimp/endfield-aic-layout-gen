"""Import an exact completed curriculum-v2 boundary without reselection or reset."""
from copy import deepcopy
from pathlib import Path

from blueprint_ppo_store import checked, load, read, reference, save, sha, write
from blueprint_ppo_update import digest, require
from declarative_learning_continue import link
from declarative_learning_runtime import CHECKPOINT, account_write


def parent(source, config):
    source=Path(source).resolve();launch=read(source/'launch.json');old=launch['config']
    seed=config['seeds'][0];directory=source/f'seed-{seed}';job=read(directory/'state.json')
    ordinal=config['training']['first_update']
    require(read(source/'state.json')['status']=='complete' and job['status']=='complete' and job['update']==ordinal,
        'declarative.duration-parent: completed registered parent boundary required')
    saved=load(checked(job['latest_checkpoint'],directory))
    require(saved['schema']==CHECKPOINT and saved['protocol_sha256']==sha(source/'launch.json') and
        saved['seed']==seed and saved['update']==ordinal and saved['model_sha256']==digest(saved['model']),
        'declarative.duration-parent: checkpoint identity mismatch')
    require(len(job['records'])==ordinal and len(job['inputs'])==ordinal*config['training']['streams']//2,
        'declarative.duration-parent: incomplete parent record/input coverage')
    require([s['index'] for s in saved['streams']]==list(range(config['training']['streams'])) and
        all(s['native'] is None and s['next_start'] is not None for s in saved['streams']) and not saved['buffer']['active'],
        'declarative.duration-parent: complete streams with selected next starts required')
    for key in ('ppo','actions','native','strata','device','execution','start_curriculum','panel'):
        require(config[key]==old[key], f'declarative.duration-parent: changed learning contract {key}')
    for key in ('streams','chunk_actions','episode_actions','elapsed_ms'):
        require(config['training'][key]==old['training'][key], f'declarative.duration-parent: changed horizon {key}')
    for key in ('schema','kind','spec','seed_base','canonical_streams','max_attempts','episode_setup_seconds'):
        require(config['training_input'][key]==old['training_input'][key], f'declarative.duration-parent: changed input contract {key}')
    for key in ('sampler','coefficient','max_sequences','max_labels'):
        require(config['success'][key]==old['success'][key], f'declarative.duration-parent: changed replay setting {key}')
    for key in ('replicates','max_actions','elapsed_ms','action_milestones'):
        require(config['evaluation'][key]==old['evaluation'][key], f'declarative.duration-parent: changed evaluation setting {key}')
    return dict(source=source,directory=directory,launch=launch,job=job,saved=saved,
        binding=dict(directory=str(source),launch=reference(source/'launch.json',source),
            checkpoint=dict(directory=str(directory),**job['latest_checkpoint']),seed=seed,update=ordinal))


def initialize(value, launch, output):
    source=value['directory'];old=value['job'];saved=deepcopy(value['saved']);ordinal=saved['update']
    directory=Path(output)/f'seed-{saved["seed"]}';directory.mkdir()
    job=dict(schema='declarative-learning-state-v2',status='ready',update=ordinal,seconds=0.,records=[],
        inputs=deepcopy(old['inputs']),source=value['binding'],first_update=ordinal,curriculum_plans={})
    needed={n for e in saved['buffer']['entries'].values() for n in e['episode_chunks']}|{ordinal}
    for ref in old['records'][:ordinal]:
        record=read(checked(ref,source));link(source/ref['path'],directory/ref['path'],job);job['records'].append(ref)
        if record['update'] in needed:
            checked(record['chunk'],source);link(source/record['chunk']['path'],directory/record['chunk']['path'],job)
    require(len(job['records'])==ordinal, 'declarative.duration-parent: missing ordered prefix records')
    wave=saved['streams'][0]['episode']+1;plan_ref=old['curriculum_plans'][str(wave)]
    plan=read(checked(plan_ref,source))
    require(plan['slots']==[s['next_start'] for s in saved['streams']], 'declarative.duration-parent: selected next wave differs')
    for slot in plan['slots']:
        if slot['depth']:checked(slot['input'],Path(slot['input']['directory']))
    link(source/plan_ref['path'],directory/plan_ref['path'],job);job['curriculum_plans'][str(wave)]=plan_ref
    initial=next(ref for ref in old['evaluation'] if ref['update']==ordinal)
    job['evaluation']=[deepcopy(initial)];job['random']=[]
    checked(initial,source);link(source/initial['path'],directory/initial['path'],job)
    for name in old.get('retained_files',{}):
        path=Path(name)
        if path.parent==source/'evaluation'/f'{ordinal:04d}' and path.name.endswith('.pt.gz'):
            link(path,directory/path.relative_to(source),job)
    # Native states and selected starts keep their exact original binding. The
    # new input store is used only when a later fresh episode is actually opened.
    saved.update(protocol_sha256=sha(Path(output)/'launch.json'),continuation_source=value['binding'])
    target=directory/f'initial/continued-{ordinal:04d}.pt.gz';save(target,saved);account_write(job,target)
    job['latest_checkpoint']=reference(target,directory);job['checkpoint_chunks']={str(ordinal):sorted(needed)}
    write(directory/'state.json',job)
    return dict(seed=saved['seed'],update=ordinal,source_chunks=len(needed),next_plan=plan_ref,
        preserved_sha256={k:digest(saved[k]) for k in ('model','actor_optimizer','critic_optimizer','sampler_rng','shuffle_rng','buffer','streams')},
        parent=value['binding'],new_checkpoint=job['latest_checkpoint'])
