"""Cold mask-arm preparation and normal-run execution; no learned-state import."""
import argparse
from copy import deepcopy
import os
from pathlib import Path
import shutil
import sys
import time

BASE = Path(__file__).resolve().parents[1]
sys.path[:0] = [str(BASE),str(BASE/'diagnostics')]

from blueprint_execution_resources import cpu_allocation
from blueprint_ppo_store import ROOT, checked, load, read, reference, save, sha, write
from blueprint_ppo_update import digest, require
from declarative_band_mask import MODE
from declarative_execution_migrate import relocate_inputs
from declarative_learning_launch import sources, validate
from declarative_learning_runtime import Learner, account_write
from declarative_quality_cold import FIELDS, bank_contract, bound, new_bank, zero_proof

FIXED = Path(__file__).with_name('declarative-mask-training.json')


def protocol():
    value = read(FIXED)
    require(value['schema']=='declarative-cold-mask-study-v1' and value['initialization_seeds']==[71,113]
        and value['variants']==['original','masked'] and value['training_updates']==128 and value['readiness_updates']==4
        and value['action_replicates']==4 and value['max_training_seconds']==9000,
        'cold-mask.protocol: changed registered comparison')
    return value


def source_binding(config, library):
    result = sources(config,library)
    paths = [FIXED,*(Path(__file__).with_name(name) for name in
        ('cold_mask_arm.py','cold_mask_study.py','cold_mask_evaluate.py','band_mask_worker.py','band_mask_policy.py'))]
    result['files_sha256'].update({str(p.relative_to(ROOT)):sha(p) for p in paths})
    result['code_sha256'] = digest(result['files_sha256'])
    return result


def configuration(seed, variant, output, reservation, *, readiness=False, seconds=2700):
    f = protocol()
    require(seed in f['initialization_seeds'] and variant in f['variants'] and 30<seconds<=2700,
        'cold-mask.configuration: invalid seed/condition/allowance')
    c = deepcopy(f['numerical_base']);c['seeds']=[seed]
    c['actions'].pop('masking',None)
    if variant=='masked':c['actions']['masking']=MODE
    c['training'].update(first_update=0,updates=f['readiness_updates'] if readiness else f['training_updates'])
    c['training_input'].update(directory=str(Path(output).resolve()/'training-inputs'),
        seed_base=f['readiness_generation_seed'] if readiness else f['training_generation_seed'],geometry_reservation=reservation)
    c['retention'].update(checkpoints=[0,1,2,4] if readiness else [0,128],training_traces=[],evaluation_traces=[])
    c['evaluation'].update(at=[0],interval_updates=32,replicates=2)
    grace=min(300.,seconds/3)
    c['duration'].update(seconds=seconds-grace,finish_grace_seconds=grace)
    c['limits'].update(arm_seconds=seconds,matrix_seconds=seconds)
    validate(c)
    return c


def reserve(panels):
    identities=set()
    for path in panels:
        value=read(path)
        require(value['status']=='complete','cold-mask.panel: incomplete reserved cohort')
        identities.update(c['geometry_identity'] for c in value['cases'])
    require(bool(identities),'cold-mask.panel: nonempty reservation required')
    return dict(schema='blueprint-geometry-reservation-v1',geometry_sha256=sorted(identities))


def prepare(output, seed, variant, panel, shared, reservation, library, verification, *, readiness=False, seconds=2700):
    output=Path(output).resolve()
    require(not output.exists(),f'cold-mask.output: new arm directory required: {output}')
    config=configuration(seed,variant,output,reservation,readiness=readiness,seconds=seconds)
    source=source_binding(config,library);allocation=cpu_allocation(config['execution_options']['workers'])
    if not readiness:
        receipt=read(verification)
        require(receipt['schema']=='cold-mask-readiness-v1' and receipt['status']=='passed' and receipt['protocol']==bound(FIXED)
            and receipt['source_sha256']==source['code_sha256'] and receipt['allocation']['workers']==allocation['workers'],
            'cold-mask.readiness: matched completed normal-execution gate required')
    contract,catalogs=bank_contract(config,library)
    require(read(Path(shared)/'manifest.json')==contract and read(Path(shared)/'catalogs.json')==catalogs,
        'cold-mask.bank: paired fresh-input contract mismatch')
    staging=output.with_name(output.name+'.staging')
    require(not staging.exists(),f'cold-mask.output: stale staging directory: {staging}')
    staging.mkdir(parents=True)
    try:
        relocate_inputs(shared,staging/'training-inputs',config)
        origin=dict(schema='cold-mask-origin-v1',directory=str(output),seed=seed,variant=variant,
            readiness=readiness,protocol=bound(FIXED),reservation_sha256=digest(reservation))
        launch=dict(schema='declarative-learning-launch-v1',config=config,source=source,allocation=allocation,
            panel=bound(panel),verification=bound(verification),batch_experiment=True,cold_origin=origin,created_unix_seconds=time.time())
        write(staging/'launch.json',launch);identity=sha(staging/'launch.json')
        learner=Learner(seed,config,identity);saved=learner.checkpoint(None)
        job=dict(schema='declarative-learning-state-v1',status='ready',update=0,seconds=0.,records=[],inputs={},evaluation=[])
        empty=zero_proof(saved,job);owner=staging/f'seed-{seed}';path=owner/'checkpoints/0000.pt.gz'
        save(path,saved);account_write(job,path)
        job['retained_files']={str(output/f'seed-{seed}'/Path(k).relative_to(owner)):v for k,v in job['retained_files'].items()}
        job['latest_checkpoint']=reference(path,owner);write(owner/'state.json',job)
        proof=dict(origin=origin,launch_sha256=identity,endpoint=config['training']['updates'],empty_state=empty)
        write(staging/'cold-origin.json',proof)
        write(staging/'state.json',dict(schema='declarative-learning-matrix-v1',status='ready',seconds=0.,launch_sha256=identity,jobs={}))
        staging.rename(output)
        return proof
    except BaseException:
        shutil.rmtree(staging)
        raise


def endpoint(directory, *, complete=True):
    directory=Path(directory);launch=read(directory/'launch.json');proof=read(directory/'cold-origin.json')
    require(sha(directory/'launch.json')==proof['launch_sha256'] and launch['cold_origin']==proof['origin']
        and proof['origin']['protocol']==bound(FIXED),'cold-mask.origin: changed cold origin/launch/protocol')
    owner=directory/f'seed-{proof["origin"]["seed"]}';job=read(owner/'state.json')
    matrix=read(directory/'state.json');saved=load(checked(job['latest_checkpoint'],owner))
    require(saved['protocol_sha256']==proof['launch_sha256'] and saved['update']==job['update']
        and saved['seed']==proof['origin']['seed'] and digest(saved['model'])==saved['model_sha256'],
        'cold-mask.checkpoint: model/origin/update binding mismatch')
    if complete:
        require(matrix['status']==job['status']=='complete' and job['update']==proof['endpoint'],
            'cold-mask.endpoint: incomplete scientific allocation')
    return dict(directory=directory,owner=owner,launch=launch,proof=proof,job=job,matrix=matrix,saved=saved)


def run(directory, environment, stop_after=None):
    import declarative_learning_run as runner
    from declarative_duration_control import requested_stop
    directory=Path(directory);value=endpoint(directory,complete=False)
    if value['saved']['update']==0:
        require({k:digest(value['saved'][k]) for k in FIELDS}==value['proof']['empty_state'],
            'cold-mask.initializer: empty initializer changed')
    require(stop_after is None or value['proof']['origin']['readiness'],
        'cold-mask.stop: update-based diagnostic stop is restricted to readiness')
    owner=value['owner'];del value
    with requested_stop() as stop:
        return runner.run(directory,environment,lambda:stop() or
            (stop_after is not None and read(owner/'state.json')['update']>=stop_after))


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--environment',type=Path,required=True)
    parser.add_argument('--stop-after',type=int)
    args=parser.parse_args()
    print(run(args.output,args.environment,args.stop_after)['status'])
