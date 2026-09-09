"""Freeze full-factory reset cases and train explicit on-policy return learners."""
import argparse
from collections import Counter
from pathlib import Path
import random
import time

import torch

from full_factory_curriculum import source_manifest
from full_factory_large_learning import read,unpack,verify_sources
from full_factory_on_policy import on_policy_rollout,replay_gradients,relative_returns,parameter_hash
from full_factory_policy import SCHEMA,FullFactoryPolicy
from full_factory_screen import sha,write


def config_check(c):
    if c.get('schema_version')!=1 or c.get('initializations')!=['random','imitation-warm']:
        raise ValueError('on-policy.config-schema-initializations')
    bounds=dict(groups=1024,group_episodes=16,horizon=128,episode_seconds=300,route_work=100000,arm_seconds=7200,
        evaluation_seconds=7200,observation_array_bytes=512*1024*1024,episode_state_bytes=512*1024*1024)
    for name,cap in bounds.items():
        if type(c.get(name)) is not int or not 0<c[name]<=cap:
            raise ValueError(f'on-policy.config-bound: {name}')
    if c['group_episodes']<2:
        raise ValueError('on-policy.config-group-episodes')
    for name in ['seed','training_seed_base']:
        if type(c.get(name)) is not int or not 0<=c[name]<2**31:
            raise ValueError(f'on-policy.config-seed: {name}')
    for name in ['learning_rate','temperature','gradient_clip']:
        if type(c.get(name)) not in (float,int) or not 0<c[name]<=1:
            raise ValueError(f'on-policy.config-scalar: {name}')
    if type(c.get('entropy_coefficient')) not in (float,int) or not 0<=c['entropy_coefficient']<=.1:
        raise ValueError('on-policy.config-entropy')
    for name in ['balanced','memory']:
        if type(c.get(name)) is not bool:
            raise ValueError(f'on-policy.config-boolean: {name}')
    banks=c.get('evaluation_banks')
    if not isinstance(banks,list) or not banks or any(type(b) is not int or b<0 for b in banks) or len(set(banks))!=len(banks):
        raise ValueError('on-policy.config-evaluation-banks')


def audited(screen):
    p,r,a=(read(screen/n) for n in ['protocol.json','report.json','audit.json'])
    if r['status']!='complete' or a['status']!='passed' or a['report_sha256']!=sha(screen/'report.json') or r['protocol_sha256']!=sha(screen/'protocol.json'):
        raise ValueError(f'on-policy.source-audit: {screen}')
    verify_sources(p)
    return p,r,a


def group_schedule(cases,config):
    order=list(cases)
    random.Random(config['seed']).shuffle(order)
    return [dict(group=g,case=order[g%len(order)],sample_seeds=[config['training_seed_base']+g*config['group_episodes']+i
        for i in range(config['group_episodes'])]) for g in range(config['groups'])]


def prepare(args):
    c=read(args.config)
    config_check(c)
    old,_,old_audit=audited(args.retention)
    preparation,prepared,_=audited(args.preparation)
    if prepared['prepared']!=len(preparation['cases']) or any(r['status']!='contraction_exhausted' for r in prepared['rows']):
        raise ValueError('on-policy.prepared-reset-membership')
    cases=[dict(case,transfer_group='old-controls') for case in old['cases']]
    raw=[dict(case,transfer_group='varied-training-raw') for case in preparation['cases']]
    dense=[dict(r['prepared'],transfer_group='varied-training-dense') for r in prepared['rows']]
    if any(case['split']!='train' for case in raw+dense):
        raise ValueError('on-policy.nontraining-reset')
    cases+=raw+dense
    protocols=[old,preparation]
    for screen,group in [(args.fresh,'opened-fresh'),(args.bottled,'opened-bottling')]:
        p,_,_=audited(screen)
        protocols.append(p)
        cases+=[dict(case,transfer_group=group) for case in p['cases']]
    if len({case['id'] for case in cases})!=len(cases):
        raise ValueError('on-policy.duplicate-case')
    catalogs={k:sha(f) for k,f in read(old['catalogs']).items()}
    if any({k:sha(f) for k,f in read(p['catalogs']).items()}!=catalogs for p in protocols):
        raise ValueError('on-policy.catalog-mismatch')
    warm=args.retention/f'learned-{c["seed"]}.pt'
    if sha(warm)!=old_audit['checkpoints'][f'learned-{c["seed"]}']:
        raise ValueError('on-policy.warm-start-checkpoint-audit')
    checkpoint=torch.load(warm,map_location='cpu',weights_only=False)
    if checkpoint['schema']!=SCHEMA or checkpoint['step']!=old['config']['updates'] or checkpoint['protocol_sha256']!=sha(args.retention/'protocol.json'):
        raise ValueError('on-policy.warm-start-contract')
    sources={}
    for p in protocols:
        sources.update(p['sources'])
    files=[args.config,warm,Path(preparation['future_evaluation'])]
    for screen in [args.retention,args.preparation,args.fresh,args.bottled]:
        files += [screen/n for n in ['protocol.json','report.json','audit.json']]
    for case in cases:
        for key,name in [('initial','initial.json'),('request','request.json'),('original','original-request.json')]:
            if sha(case[key])!=case['files_sha256'][name]:
                raise ValueError(f'on-policy.case-hash: {case[key]}')
            files.append(Path(case[key]))
    sources.update({str(f.resolve()):sha(f) for f in files})
    training_cases=[case['id'] for case in raw+dense]
    p=dict(schema_version=1,experiment='on-policy-complete-factory-relative-return',config=c,cases=cases,training_cases=training_cases,
        schedule=group_schedule(training_cases,c),catalogs=old['catalogs'],native_library=old['native_library'],
        schema=SCHEMA,depth=old['depth'],width=old['width'],warm_checkpoint=str(warm.resolve()),retention=str(args.retention.resolve()),
        preparation=str(args.preparation.resolve()),future_evaluation=preparation['future_evaluation'],sources=sources,source_sha256=source_manifest(),
        learning_method='On-policy relative-return REINFORCE with separately measured entropy regularization; no action labels',
        reset_curriculum='Five original and five contraction-exhausted whole training factories; preparation trace is not consumed by training')
    verify_sources(p)
    args.output.mkdir()
    write(args.output/'protocol.json',p)
    print(dict(status='frozen',training_cases=len(training_cases),known_evaluation_cases=len(cases),
        training_actions_per_arm=c['groups']*c['group_episodes']*c['horizon']),flush=True)


def checkpoint(path,p,model,optimizer,arm,next_group,protocol_file):
    torch.save(dict(schema=SCHEMA,depth=p['depth'],width=p['width'],initialization=arm,seed=p['config']['seed'],
        learning_method=p['learning_method'],next_group=next_group,policy=model.state_dict(),optimizer=optimizer.state_dict(),rng=torch.get_rng_state(),
        policy_sha256=parameter_hash(model),protocol_sha256=sha(protocol_file)),path)


def train(args):
    started=time.perf_counter()
    torch.set_num_threads(1)
    torch.use_deterministic_algorithms(True)
    p=read(args.screen/'protocol.json')
    config_check(p['config'])
    verify_sources(p)
    if any((args.screen/n).exists() for n in ['training-report.json','training-progress.json','partial.json']):
        raise ValueError('on-policy.existing-training: use a fresh screen')
    c=p['config']
    if p['schedule']!=group_schedule(p['training_cases'],c):
        raise ValueError('on-policy.group-schedule')
    cases={case['id']:case for case in p['cases']}
    rows,groups,arms=[],[],[]
    active_arm=active_group=None
    try:
        for arm in c['initializations']:
            active_arm=arm
            began=time.perf_counter()
            def guard():
                if time.perf_counter()-began>=c['arm_seconds']:
                    raise TimeoutError(f'on-policy.arm-budget: {arm}')
            torch.manual_seed(c['seed'])
            model=FullFactoryPolicy(p['depth'],p['width'])
            if arm=='imitation-warm':
                warm=torch.load(p['warm_checkpoint'],map_location='cpu',weights_only=False)
                model.load_state_dict(warm['policy'],strict=True)
            optimizer=torch.optim.Adam(model.parameters(),lr=c['learning_rate'])
            arm_directory=args.screen/arm
            arm_directory.mkdir()
            checkpoint(arm_directory/'initial.pt',p,model,optimizer,arm,0,args.screen/'protocol.json')
            arm_groups=[]
            for item in p['schedule']:
                guard()
                active_group=item['group']
                group_started=time.perf_counter()
                group_directory=arm_directory/f'group-{item["group"]:04d}'
                group_directory.mkdir()
                episode_rows=[]
                behavior_sha256=parameter_hash(model)
                for attempt,seed in enumerate(item['sample_seeds']):
                    guard()
                    row,summary=on_policy_rollout(p,cases[item['case']],model,seed,group_directory/f'episode-{attempt}')
                    row.update(initialization=arm,group=item['group'],attempt=attempt,phase='training')
                    rows.append(row)
                    episode_rows.append(row)
                    write(args.screen/'training-progress.json',dict(status='incomplete',active_arm=arm,active_group=item['group'],rows=rows,groups=groups))
                    if not row['budget_complete']:
                        raise TimeoutError(f'on-policy.incomplete-training-episode: {row["directory"]}')
                if parameter_hash(model)!=behavior_sha256:
                    raise ValueError('on-policy.group-policy-mutated')
                scores=[r['best_metrics']['score'] for r in episode_rows]
                returns=relative_returns(scores)
                optimizer.zero_grad(set_to_none=True)
                gradient=replay_gradients(p,cases[item['case']],model,episode_rows,returns,guard,unpack)
                guard()
                clipped=float(torch.nn.utils.clip_grad_norm_(model.parameters(),c['gradient_clip']))
                if not torch.isfinite(torch.tensor(clipped)):
                    raise ValueError('on-policy.nonfinite-clipping')
                optimizer.step()
                after_sha256=parameter_hash(model)
                checkpoint(group_directory/'checkpoint.pt',p,model,optimizer,arm,item['group']+1,args.screen/'protocol.json')
                record=dict(initialization=arm,group=item['group'],case=item['case'],sample_seeds=item['sample_seeds'],scores=scores,returns=returns,
                    **gradient,pre_clip_gradient_norm=clipped,policy_before_sha256=behavior_sha256,policy_after_sha256=after_sha256,
                    parameter_changed=after_sha256!=behavior_sha256,seconds=time.perf_counter()-group_started,
                    checkpoint=str((group_directory/'checkpoint.pt').resolve()),checkpoint_sha256=sha(group_directory/'checkpoint.pt'))
                groups.append(record)
                arm_groups.append(record)
                write(group_directory/'group.json',record)
                write(args.screen/'training-progress.json',dict(status='incomplete',active_arm=arm,active_group=item['group'],rows=rows,groups=groups))
                if (item['group']+1)%4==0:
                    print(dict(initialization=arm,groups=item['group']+1,nonflat=sum(not g['flat_returns'] for g in arm_groups),
                        reward_gradient=record['reward_gradient_norm'],entropy_gradient=record['entropy_gradient_norm'],seconds=time.perf_counter()-began),flush=True)
            checkpoint(arm_directory/'final.pt',p,model,optimizer,arm,c['groups'],args.screen/'protocol.json')
            arms.append(dict(initialization=arm,groups=len(arm_groups),seconds=time.perf_counter()-began,
                nonflat_groups=sum(not g['flat_returns'] for g in arm_groups),reward_gradient_groups=sum(g['reward_gradient_norm']>0 for g in arm_groups),
                initial=str((arm_directory/'initial.pt').resolve()),initial_sha256=sha(arm_directory/'initial.pt'),
                final=str((arm_directory/'final.pt').resolve()),final_sha256=sha(arm_directory/'final.pt'),parameters=sum(v.numel() for v in model.parameters())))
        verify_sources(p)
        write(args.screen/'training-report.json',dict(schema_version=1,status='complete',protocol_sha256=sha(args.screen/'protocol.json'),
            seconds=time.perf_counter()-started,arms=arms,rows=rows,groups=groups,teacher_labels_used=False,future_evaluation_opened=False))
    except BaseException as error:
        write(args.screen/'partial.json',dict(schema_version=1,status='incomplete',code='on-policy.training-failed',severity='error',stage='training',
            initialization=active_arm,group=active_group,message=str(error),rows=rows,groups=groups,arms=arms))
        raise


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    sub=parser.add_subparsers(dest='command',required=True)
    freeze=sub.add_parser('prepare')
    for name in ['retention','preparation','fresh','bottled','config','output']:
        freeze.add_argument('--'+name,required=True,type=Path)
    sub.add_parser('train').add_argument('--screen',required=True,type=Path)
    args=parser.parse_args()
    (prepare if args.command=='prepare' else train)(args)
