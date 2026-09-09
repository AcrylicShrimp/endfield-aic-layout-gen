"""Read-only reward accounting and fixed-observation policy drift diagnosis."""
import argparse
from collections import Counter, defaultdict
from pathlib import Path

import torch

from full_factory_environment import FactoryRuntime, digest
from full_factory_large_learning import read, unpack, verify_sources
from full_factory_learnability_audit import observation_key
from full_factory_on_policy import parameter_hash
from full_factory_on_policy_result import load_result
from full_factory_policy import FullFactoryPolicy, observe
from full_factory_screen import sha, write
from full_factory_selection import SelectionMemory, action_key, physical_key, distribution


def reward_accounting(result, proof, sources):
    output = {}
    for arm in result['arms']:
        name = arm['initialization']
        groups = {g['group']:g for g in result['groups'] if g['initialization']==name}
        stats = defaultdict(lambda:dict(groups=0,nonflat_groups=0,episodes=0,selected=0,kinds=Counter(),statuses=Counter(),
                                       positive_episodes=0,positive_contract_only=0,positive_with_improving_pose=0))
        for group in groups.values():
            role = 'prepared' if group['case'].endswith('--contraction-exhausted') else 'original'
            stats[role]['groups'] += 1
            stats[role]['nonflat_groups'] += not group['flat_returns']
        for row in result['rows']:
            if row['initialization']!=name or row['group'] not in groups:
                continue
            role = 'prepared' if row['case'].endswith('--contraction-exhausted') else 'original'
            stat = stats[role]
            stat['episodes'] += 1
            stat['selected'] += row['selected']
            stat['kinds'].update(row['kinds'])
            stat['statuses'].update(row['statuses'])
            if groups[row['group']]['returns'][row['attempt']]>0:
                stat['positive_episodes'] += 1
                path = Path(row['directory'])/'episode.json.gz'
                evidence = next(r for r in proof['rows'] if r['key']==f'{name}:{row["group"]}:{row["attempt"]}')
                if sha(path)!=evidence['episode_sha256']:
                    raise ValueError(f'on-policy.diagnosis-reward-source: {path}')
                sources[str(path.resolve())] = evidence['episode_sha256']
                episode = unpack(path)
                improved = {step['action']['kind'] for step in episode['trace'] if step['status']=='improved'}
                stat['positive_contract_only'] += improved=={'contract'}
                stat['positive_with_improving_pose'] += 'pose' in improved
        output[name] = dict(stats)
    return output


def first_step_accounting(result):
    output = {}
    for arm in result['arms']:
        group = next(g for g in result['groups'] if g['initialization']==arm['initialization'] and g['group']==0)
        before = torch.load(arm['initial'],map_location='cpu',weights_only=False)
        after = torch.load(group['checkpoint'],map_location='cpu',weights_only=False)
        if before['optimizer']['state'] or len(after['optimizer']['param_groups'])!=1:
            raise ValueError('on-policy.diagnosis-first-adam-state')
        config = after['optimizer']['param_groups'][0]
        if config['weight_decay'] or config['amsgrad'] or config.get('maximize') or len(config['params'])!=len(before['policy']):
            raise ValueError('on-policy.diagnosis-adam-variant')
        beta1,beta2 = config['betas']
        observed, predicted = [], []
        for (name,value),parameter in zip(before['policy'].items(),config['params']):
            state = after['optimizer']['state'][parameter]
            if float(state['step'])!=1:
                raise ValueError('on-policy.diagnosis-adam-step')
            observed.append((after['policy'][name].double()-value.double()).reshape(-1))
            mean = state['exp_avg'].double()/(1-beta1)
            variance = state['exp_avg_sq'].double()/(1-beta2)
            predicted.append((-config['lr']*mean/(variance.sqrt()+config['eps'])).reshape(-1))
        observed,predicted = torch.cat(observed),torch.cat(predicted)
        error = float((observed-predicted).abs().max())
        if error>1e-6:
            raise ValueError('on-policy.diagnosis-adam-update-mismatch')
        output[arm['initialization']] = dict(initial_optimizer_entries=0,parameters=observed.numel(),learning_rate=config['lr'],
            reward_gradient_norm=group['reward_gradient_norm'],entropy_gradient_norm=group['entropy_gradient_norm'],
            median_absolute_parameter_delta=float(observed.abs().median()),
            fraction_delta_above_90_percent_learning_rate=float((observed.abs()>.9*config['lr']).double().mean()),
            maximum_recorded_moment_formula_error=error)
    return output


def run(args):
    torch.set_num_threads(1)
    torch.use_deterministic_algorithms(True)
    protocol, result = load_result(args.training)
    proof = read(args.training/'training-audit.json')
    if proof['status']!='passed' or proof['report_sha256']!=sha(args.training/'training-result.json'):
        raise ValueError('on-policy.diagnosis-training-audit')
    selected = [next(r for r in result['rows'] if r['initialization']=='random' and r['group']==g and r['attempt']==0)
                for g in range(10)]
    if {r['case'] for r in selected}!=set(protocol['training_cases']):
        raise ValueError('on-policy.diagnosis-panel-membership')
    sources = {str((args.training/name).resolve()):sha(args.training/name) for name in
               ('protocol.json','training-result.json','training-audit.json','checkpoint-audit.json')}
    sources[str(Path(__file__).resolve())] = sha(__file__)
    models = {}
    parameter_changes = {}
    checkpoints = []
    for arm in result['arms']:
        name = arm['initialization']
        models[name] = {}
        for update in range(arm['groups']+1):
            path = Path(arm['initial']) if update==0 else Path(arm['terminal']) if update==arm['groups'] else args.training/name/f'group-{update-1:04d}/checkpoint.pt'
            value = torch.load(path,map_location='cpu',weights_only=False)
            model = FullFactoryPolicy(protocol['depth'],protocol['width'])
            model.load_state_dict(value['policy'],strict=True)
            identity = parameter_hash(model)
            if identity!=value['policy_sha256'] or value['next_group']!=update:
                raise ValueError(f'on-policy.diagnosis-checkpoint: {path}')
            models[name][update] = model
            sources[str(path.resolve())] = sha(path)
            checkpoints.append(dict(initialization=name,update=update,path=str(path.resolve()),policy_sha256=identity))
        initial = models[name][0].state_dict()
        terminal = models[name][arm['groups']].state_dict()
        initial_norm = sum(v.double().square().sum().item() for v in initial.values())**.5
        difference = sum((terminal[k].double()-v.double()).square().sum().item() for k,v in initial.items())**.5
        parameter_changes[name] = dict(initial_l2=initial_norm,delta_l2=difference,relative_l2=difference/initial_norm,
                                       max_absolute_change=max((terminal[k]-v).abs().max().item() for k,v in initial.items()))
    for row in selected:
        evidence = next(r for r in proof['rows'] if r['key']==f'random:{row["group"]}:0')
        for name in ('states','episode'):
            path = Path(row['directory'])/f'{name}.json.gz'
            if sha(path)!=evidence[f'{name}_sha256']:
                raise ValueError(f'on-policy.diagnosis-source-state: {path}')
            sources[str(path.resolve())] = sha(path)
    reward_stats = reward_accounting(result,proof,sources)
    first_steps = first_step_accounting(result)
    args.output.mkdir()
    write(args.output/'protocol.json',dict(schema_version=1,sources=sources,checkpoints=checkpoints,
                                          panel=[dict(case=r['case'],group=r['group'],directory=r['directory']) for r in selected],
                                          action_selection_performed=False,training_performed=False,
                                          runtime_source_sha256=protocol['source_sha256']))
    rows = []
    runtime = FactoryRuntime(protocol['native_library'],protocol['catalogs'])
    try:
        for source in selected:
            record = unpack(Path(source['directory'])/'states.json.gz')[0]
            episode = unpack(Path(source['directory'])/'episode.json.gz')
            state = record['state']
            if digest(state['current'])!=episode['trace'][0]['current_before_sha256']:
                raise ValueError('on-policy.diagnosis-current-identity')
            description = runtime.rpc('describe',state['current'],options={})
            observation = observe(state['current'],description,state,1)
            if observation_key(observation)!=record['observation_sha256']:
                raise ValueError('on-policy.diagnosis-observation-identity')
            physical = physical_key(state['current'])
            keys = [action_key(a,description,state['current'],physical) for a in description['actions']]
            allowed, exhausted = SelectionMemory(state['current']).mask(physical,keys,description['actions'],protocol['config']['memory'])
            if allowed!=record['allowed'] or exhausted!=episode['trace'][0]['memory_exhausted']:
                raise ValueError('on-policy.diagnosis-mask-identity')
            for name, versions in models.items():
                initial = None
                previous = None
                for update, model in versions.items():
                    with torch.no_grad():
                        probabilities = distribution(model(observation)/protocol['config']['temperature'],description['actions'],allowed,protocol['config']['balanced'])
                        if initial is None:
                            initial = probabilities.probs.clone()
                            previous = initial
                        mass = Counter()
                        for action,probability in zip(description['actions'],probabilities.probs.tolist()):
                            mass[action['kind']] += probability
                        rows.append(dict(case=source['case'],reset='prepared' if source['case'].endswith('--contraction-exhausted') else 'original',
                                         initialization=name,update=update,kind_probability=dict(mass),entropy=float(probabilities.entropy()),
                                         total_variation_from_initial=float((probabilities.probs-initial).abs().sum()/2),
                                         total_variation_from_previous=float((probabilities.probs-previous).abs().sum()/2),
                                         top_action_kind=description['actions'][int(probabilities.probs.argmax())]['kind'],
                                         top_action_probability=float(probabilities.probs.max()),
                                         observation_sha256=record['observation_sha256'],allowed_sha256=digest(allowed)))
                        previous = probabilities.probs.clone()
            print(dict(case=source['case'],evaluated_checkpoints=sum(map(len,models.values()))),flush=True)
    finally:
        runtime.close()
    for checkpoint in checkpoints:
        if parameter_hash(models[checkpoint['initialization']][checkpoint['update']])!=checkpoint['policy_sha256']:
            raise ValueError('on-policy.diagnosis-policy-mutated')
    if any(sha(path)!=expected for path,expected in sources.items()):
        raise ValueError('on-policy.diagnosis-source-drift')
    verify_sources(protocol)
    write(args.output/'report.json',dict(schema_version=1,status='complete',protocol_sha256=sha(args.output/'protocol.json'),
                                       parameter_changes=parameter_changes,first_step_accounting=first_steps,reward_accounting=reward_stats,rows=rows,
                                       training_performed=False,action_selection_performed=False,
                                       scope='Audited training associations and fixed-observation policy drift; no causal ablation or new rollout evaluation.'))


if __name__=='__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('training','output'):
        parser.add_argument('--'+name,required=True,type=Path)
    run(parser.parse_args())
