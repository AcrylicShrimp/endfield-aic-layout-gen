"""On-policy relative-return gradients with bounded, distribution-exact replay."""
from collections import Counter
import hashlib
import json
from pathlib import Path
import time

import numpy as np
import torch

from full_factory_environment import FactoryRuntime, FactoryEpisode, digest
from full_factory_learnability_audit import observation_key
from full_factory_policy import observe
from full_factory_screen import save_gzip
from full_factory_selection import SelectionMemory, action_key, physical_key, distribution, geometry_choice


def relative_returns(scores):
    if len(scores)<2 or any(len(s)!=3 or any(type(v) is not int or v<0 for v in s) for s in scores):
        raise ValueError('on-policy.invalid-terminal-scores')
    scores=[tuple(s) for s in scores]
    return [sum((s<other)-(s>other) for other in scores)/(len(scores)-1) for s in scores]


def parameter_hash(model):
    result=hashlib.sha256()
    for name,value in model.state_dict().items():
        result.update(name.encode())
        result.update(str((value.dtype,tuple(value.shape))).encode())
        result.update(value.detach().cpu().contiguous().numpy().tobytes())
    return result.hexdigest()


def probability_hash(probabilities):
    return hashlib.sha256(probabilities.detach().cpu().contiguous().numpy().tobytes()).hexdigest()


def gradient_norm(values):
    return float(torch.sqrt(sum(v.double().square().sum() for v in values)))


def loss_components(log_probability,entropy,advantage,episodes,horizon,entropy_coefficient):
    return -advantage*log_probability/(episodes*horizon),-entropy_coefficient*entropy/(episodes*horizon)


def add_gradients(total,values):
    for destination,value in zip(total,values):
        if value is not None:
            destination.add_(value.detach())


def on_policy_rollout(p,case,model,seed,directory,save_states=True,geometry=False):
    """No autograd or candidate-score lookahead; every selected action is executed."""
    began=time.perf_counter()
    c=p['config']
    directory.mkdir()
    runtime=FactoryRuntime(p['native_library'],p['catalogs'])
    states=[]
    env=None
    observation_seconds=selection_seconds=recording_seconds=0
    max_observation_bytes=state_bytes=0
    policy_before=parameter_hash(model) if model is not None else None
    try:
        if runtime.description_cache or runtime.description_cache_bytes:
            raise ValueError('on-policy.nonempty-reset-cache')
        env=FactoryEpisode(runtime,case,c['horizon'],c['episode_seconds'],c['route_work'])
        # The outer deadline also charges runtime startup and original admission.
        env.started=began
        memory=SelectionMemory(env.state['current'])
        current_key=best_key=physical_key(env.state['current'])
        rng=torch.Generator().manual_seed(seed)
        while not env.done:
            started=time.perf_counter()
            description=env.describe()
            obs=observe(env.state['current'],description,env.state,1-env.selected/c['horizon']) if model is not None else None
            if obs is not None:
                size=sum(v.nbytes for v in obs.values() if isinstance(v,np.ndarray))
                max_observation_bytes=max(max_observation_bytes,size)
                if size>c['observation_array_bytes']:
                    raise ValueError('on-policy.observation-array-bound')
            observation_seconds+=time.perf_counter()-started
            started=time.perf_counter()
            keys=[action_key(a,description,env.state['current'],best_key) for a in description['actions']]
            allowed,exhausted=memory.mask(current_key,keys,description['actions'],c['memory'])
            if env.done:
                break
            with torch.no_grad():
                logits=model(obs)/c['temperature'] if model is not None else torch.zeros(len(allowed))
                dist=distribution(logits,description['actions'],allowed,c['balanced'])
                index=geometry_choice(env.state['current'],description['actions'],allowed) if geometry else int(torch.multinomial(dist.probs,1,generator=rng))
                old_logp=0.0 if geometry else float(dist.log_prob(torch.tensor(index)))
                entropy=0.0 if geometry else float(dist.entropy())
                probabilities_sha256=None if geometry else probability_hash(dist.probs)
            if not np.isfinite(old_logp) or not np.isfinite(entropy):
                raise ValueError('on-policy.nonfinite-sampling')
            selection_seconds+=time.perf_counter()-started
            started=time.perf_counter()
            metrics={name:env.state[name] for name in ['initial_metrics','current_metrics','best_metrics']}
            state=dict(current=env.state['current'],**metrics)
            current_sha256=digest(env.state['current'])
            observed_sha256=observation_key(obs) if obs is not None else None
            record=dict(state=state,allowed=allowed,index=index,observation_sha256=observed_sha256,
                        old_log_probability=old_logp,probabilities_sha256=probabilities_sha256)
            if save_states:
                state_bytes+=len(json.dumps(record,separators=(',',':')).encode())
                if state_bytes>c['episode_state_bytes']:
                    raise ValueError('on-policy.episode-state-bound')
            recording_seconds+=time.perf_counter()-started
            if env.done:
                break
            before=current_key
            row=env.step(description['actions'][index])
            started=time.perf_counter()
            current_key=before if row['unchanged'] else physical_key(env.state['current'])
            unchanged,revisited=memory.record(before,current_key,keys[index])
            if row['status']=='improved':
                best_key=current_key
            row.update(vocabulary_index=index,current_before_sha256=current_sha256,current_sha256=digest(env.state['current']),
                observation_sha256=observed_sha256,old_log_probability=old_logp,sampling_entropy=entropy,probabilities_sha256=probabilities_sha256,
                allowed_sha256=digest(allowed),allowed_count=sum(allowed),memory_exhausted=exhausted,action_key=keys[index],
                physical_before=before,physical_after=current_key,physical_unchanged=unchanged,physical_revisited=revisited)
            if save_states:
                states.append(record)
            selection_seconds+=time.perf_counter()-started
        summary=env.summary()
        best=env.state['best']
        started=time.perf_counter()
        if save_states:
            save_gzip(directory/'states.json.gz',states)
        save_gzip(directory/'best.json.gz',best)
        recording_seconds+=time.perf_counter()-started
        summary.update(sample_seed=seed,config={k:c[k] for k in ['horizon','temperature','balanced','memory','episode_seconds','route_work']},
            geometry=geometry,policy_sha256=policy_before,observation_seconds=observation_seconds,selection_seconds=selection_seconds,
            recording_seconds=recording_seconds,max_observation_array_bytes=max_observation_bytes,state_json_bytes=state_bytes,
            statuses=dict(Counter(t['status'] for t in env.trace)),kinds=dict(Counter(t['action']['kind'] for t in env.trace)),
            route_expansions=sum(t['route_expansions'] for t in env.trace),elapsed_seconds=time.perf_counter()-began)
        save_gzip(directory/'episode.json.gz',summary)
    except BaseException as error:
        if env is not None:
            save_gzip(directory/'best.json.gz',env.state['best'])
            save_gzip(directory/'episode-partial.json.gz',env.summary())
            if save_states:
                save_gzip(directory/'states.json.gz',states)
        (directory/'partial.json').write_text(json.dumps(dict(status='incomplete',code='on-policy.rollout-failed',
            severity='error',stage='rollout',entity=case['id'],message=str(error)),indent=2)+'\n')
        raise
    finally:
        runtime.close()
    if model is not None and parameter_hash(model)!=policy_before:
        raise ValueError('on-policy.policy-changed-during-sampling')
    total=time.perf_counter()-began
    row=dict(case=case['id'],family=case['family'],split=case['split'],transfer_group=case['transfer_group'],instances=case['instances'],
        bank=seed,directory=str(directory.resolve()),selected=summary['selected'],initial_metrics=summary['initial_metrics'],best_metrics=summary['best_metrics'],
        seconds=total,observation_seconds=observation_seconds,selection_seconds=selection_seconds,recording_seconds=recording_seconds,
        statuses=summary['statuses'],kinds=summary['kinds'],route_expansions=summary['route_expansions'],
        improvements=dict(Counter(t['action']['kind'] for t in summary['trace'] if t['status']=='improved')),
        budget_complete=summary['selected']==c['horizon'] and total<c['episode_seconds'] and not any(t['status']=='late' for t in summary['trace']))
    return row,summary


def replay_gradients(p,case,model,episodes,returns,guard,unpack):
    """Recompute against frozen behavior weights, accumulating one step at a time."""
    c=p['config']
    if len(episodes)!=c['group_episodes'] or len(returns)!=len(episodes):
        raise ValueError('on-policy.replay-group-size')
    parameters=list(model.parameters())
    reward_gradients=[torch.zeros_like(v) for v in parameters]
    entropy_gradients=[torch.zeros_like(v) for v in parameters]
    model_hash=parameter_hash(model)
    replayed=0
    reward_loss=entropy_loss=0.0
    max_logp_error=0.0
    runtime=FactoryRuntime(p['native_library'],p['catalogs'])
    try:
        for episode_row,advantage in zip(episodes,returns):
            guard()
            directory=Path(episode_row['directory'])
            states=unpack(directory/'states.json.gz')
            trace=unpack(directory/'episode.json.gz')
            if trace['policy_sha256']!=model_hash or len(states)!=c['horizon'] or len(trace['trace'])!=len(states) or not episode_row['budget_complete']:
                raise ValueError('on-policy.replay-behavior-identity')
            memory=SelectionMemory(states[0]['state']['current'])
            best_key=physical_key(states[0]['state']['current'])
            for ordinal,(record,step) in enumerate(zip(states,trace['trace'])):
                guard()
                state=record['state']
                if digest(state['current'])!=step['current_before_sha256']:
                    raise ValueError('on-policy.replay-current-hash')
                description=runtime.rpc('describe',state['current'],options={})
                obs=observe(state['current'],description,state,1-ordinal/c['horizon'])
                if observation_key(obs)!=record['observation_sha256'] or record['observation_sha256']!=step['observation_sha256']:
                    raise ValueError('on-policy.replay-observation')
                keys=[action_key(a,description,state['current'],best_key) for a in description['actions']]
                before=physical_key(state['current'])
                allowed,exhausted=memory.mask(before,keys,description['actions'],c['memory'])
                if allowed!=record['allowed'] or digest(allowed)!=step['allowed_sha256'] or exhausted!=step['memory_exhausted']:
                    raise ValueError('on-policy.replay-history-mask')
                index=record['index']
                if index!=step['vocabulary_index'] or description['actions'][index]!=step['action'] or not allowed[index]:
                    raise ValueError('on-policy.replay-selected-action')
                dist=distribution(model(obs)/c['temperature'],description['actions'],allowed,c['balanced'])
                logp=dist.log_prob(torch.tensor(index))
                entropy=dist.entropy()
                error=abs(float(logp.detach())-record['old_log_probability'])
                max_logp_error=max(max_logp_error,error)
                if error!=0 or probability_hash(dist.probs)!=record['probabilities_sha256'] or record['probabilities_sha256']!=step['probabilities_sha256']:
                    raise ValueError('on-policy.replay-sampling-distribution')
                policy_loss,regularizer=loss_components(logp,entropy,advantage,len(episodes),c['horizon'],c['entropy_coefficient'])
                reward_loss+=float(policy_loss.detach())
                entropy_loss+=float(regularizer.detach())
                if advantage:
                    add_gradients(reward_gradients,torch.autograd.grad(policy_loss,parameters,retain_graph=c['entropy_coefficient']>0,allow_unused=True))
                if c['entropy_coefficient']:
                    add_gradients(entropy_gradients,torch.autograd.grad(regularizer,parameters,allow_unused=True))
                after=step['physical_after']
                memory.record(before,after,keys[index])
                if step['status']=='improved':
                    best_key=after
                replayed+=1
    finally:
        runtime.close()
    if parameter_hash(model)!=model_hash:
        raise ValueError('on-policy.policy-changed-before-update')
    reward_norm,entropy_norm=gradient_norm(reward_gradients),gradient_norm(entropy_gradients)
    if not any(returns) and reward_norm!=0:
        raise ValueError('on-policy.flat-return-gradient')
    for param,reward,regularizer in zip(parameters,reward_gradients,entropy_gradients):
        param.grad=reward+regularizer
    combined=gradient_norm([p.grad for p in parameters])
    if not all(np.isfinite(v) for v in [reward_loss,entropy_loss,reward_norm,entropy_norm,combined]):
        raise ValueError('on-policy.nonfinite-gradient')
    return dict(replayed_actions=replayed,max_log_probability_error=max_logp_error,reward_loss=reward_loss,entropy_loss=entropy_loss,
        reward_gradient_norm=reward_norm,entropy_gradient_norm=entropy_norm,combined_gradient_norm=combined,
        flat_returns=not any(returns),behavior_policy_sha256=model_hash)
