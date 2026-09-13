"""Fresh schema-bound learner using the established optimized PPO/SIL kernel."""
from copy import deepcopy
import time

import torch

from blueprint_duration_metrics import training_quality
from blueprint_execution_update import update_packed
from blueprint_ppo_store import load, read, reference, save, write
from blueprint_ppo_update import digest, require
from declarative_learning_buffer import Buffer
import declarative_learning_policy as policy

CHECKPOINT = 'declarative-learning-checkpoint-v2'


class Learner:
    def __init__(self, seed, config, protocol_hash, saved=None, *, device=None):
        self.seed, self.config, self.protocol_hash = seed,config,protocol_hash
        self.device = device or config['device']
        self.model,self.actor,self.critic = policy.learners(seed,config['ppo'],self.device)
        self.sampler = torch.Generator().manual_seed(1_000_000+seed)
        self.shuffle = torch.Generator().manual_seed(2_000_000+seed)
        self.ordinal, self.streams = 0,None
        self.buffer = Buffer()
        if saved is not None:
            require(saved['schema']==CHECKPOINT and saved['protocol_sha256']==protocol_hash and saved['seed']==seed,
                'declarative checkpoint schema/launch/seed mismatch')
            require(saved['device']==self.device, 'declarative resume requires its recorded device')
            self.model.load_state_dict(saved['model'],strict=True)
            self.actor.load_state_dict(deepcopy(saved['actor_optimizer']))
            self.critic.load_state_dict(deepcopy(saved['critic_optimizer']))
            self.sampler.set_state(saved['sampler_rng']); self.shuffle.set_state(saved['shuffle_rng'])
            self.ordinal,self.streams = saved['update'],deepcopy(saved['streams'])
            self.buffer = Buffer(saved['buffer'])
            require(digest(self.model.state_dict())==saved['model_sha256'], 'declarative restored weights mismatch')
            require(digest(self.actor.state_dict())==digest(saved['actor_optimizer']) and
                digest(self.critic.state_dict())==digest(saved['critic_optimizer']), 'declarative restored Adam mismatch')
            for optimizer in (self.actor,self.critic):
                require(all(g['lr']==config['ppo']['learning_rate'] for g in optimizer.param_groups), 'declarative restored learning rate')
                for parameter,state in optimizer.state.items():
                    require(all(state[k].shape==parameter.shape for k in ('exp_avg','exp_avg_sq')),
                        'declarative restored Adam parameter ownership')
        self.behavior = deepcopy(self.model).cpu().eval()

    def behavior_copy(self):
        self.behavior.load_state_dict({k:v.detach().cpu() for k,v in self.model.state_dict().items()})
        return self.behavior

    def checkpoint(self, streams):
        return dict(schema=CHECKPOINT,protocol_sha256=self.protocol_hash,seed=self.seed,device=self.device,update=self.ordinal,
            model=self.model.state_dict(),model_sha256=digest(self.model.state_dict()),actor_optimizer=self.actor.state_dict(),
            critic_optimizer=self.critic.state_dict(),sampler_rng=self.sampler.get_state(),shuffle_rng=self.shuffle.get_state(),
            streams=streams,buffer=self.buffer.snapshot())

    def update(self, payload, evidence):
        ordinal = self.ordinal+1
        require(payload['schema']==self.config['chunk_schema'], 'declarative chunk schema')
        require(payload['model_before']==digest(self.model.state_dict()), 'declarative behavior/model boundary mismatch')
        evidence.begin(ordinal,payload)
        sequences = self.buffer.eligible(evidence,ordinal,self.config['strata'])
        success = self.config['success']
        result = update_packed(self.model,self.actor,self.critic,self.shuffle,payload['samples'],payload['frontiers'],
            self.config['ppo'],sequences,success['coefficient'],success['sampler'],self.seed,ordinal,ordinal,
            self.device,likelihood=policy)
        self.buffer.mark_used(result['success']['selected_ids'])
        events = self.buffer.ingest(payload,ordinal,evidence,success,self.config['ppo']['gamma'])
        self.ordinal = ordinal
        work = {key:sum(s['outcome']['details'][key] for s in payload['samples'])
                for key in ('route_expansions','preflight_expansions','route_calls','validations')}
        return dict(schema='declarative-learning-update-v1',update=ordinal,actions=len(payload['samples']),
            area_reward=sum(s['reward'] for s in payload['samples']),collection_seconds=payload['collection_seconds'],
            quality=training_quality(payload),ppo=result,buffer=self.buffer.summary(sequences),events=events,work=work,
            curriculum_completed=[dict(stream=r['stream'],episode=r['episode'],
                depth=r['training_input']['start_curriculum']['depth'],
                start_area=r['initial_score'][0],best_area=r['result']['best_metrics']['score'][0],
                root_area=r['training_input']['start_curriculum']['root']['area'],
                start_area_reduction_percent=100*(1-r['result']['best_metrics']['score'][0]/r['initial_score'][0]),
                root_area_reduction_percent=100*(1-r['result']['best_metrics']['score'][0]/r['training_input']['start_curriculum']['root']['area']))
                for r in payload['completed']] if self.config.get('start_curriculum') else [])


def account_write(job, path):
    size = path.stat().st_size
    key = str(path)
    ledger = job.setdefault('retained_files',{})
    job['retained_bytes'] = job.get('retained_bytes',0)+size-ledger.get(key,0)
    ledger[key] = size


def account_remove(job, path):
    key = str(path)
    size = job['retained_files'].pop(key)
    path.unlink()
    job['retained_bytes'] -= size
    job.setdefault('removed_files',[]).append(dict(path=key,bytes=size))


def publish(learner, payload, record, streams, directory, job, *, workers=None):
    began = time.perf_counter()
    ordinal = learner.ordinal
    chunk = directory/f'chunks/{ordinal:04d}.pt.gz'
    checkpoint = directory/f'checkpoints/{ordinal:04d}.pt.gz'
    save(chunk,payload)
    chunk_ref=reference(chunk,directory)
    if learner.config.get('start_curriculum') and payload['completed']:
        from declarative_learning_curriculum import next_plan
        require(workers is not None, 'declarative.curriculum-publication: workers required for next-start assignment')
        plan,ref=next_plan(learner.config,learner.seed,directory,payload,chunk_ref)
        workers.assign_starts(plan['slots'])
        streams=workers.snapshot()
        record['curriculum_next_plan']=ref
        job.setdefault('curriculum_plans',{})[str(plan['wave'])]=ref
        account_write(job,directory/ref['path'])
        for slot in plan['slots']:
            if slot['depth']:account_write(job,directory/slot['input']['path'])
    save(checkpoint,learner.checkpoint(streams))
    account_write(job,chunk); account_write(job,checkpoint)
    record.update(chunk=chunk_ref,checkpoint=reference(checkpoint,directory),
        publication_seconds=time.perf_counter()-began)
    path = directory/f'updates/{ordinal:04d}.json'
    write(path,record); account_write(job,path)
    require(len(job['records'])==ordinal-1, 'declarative durable update publication order')
    job['records'].append(reference(path,directory))
    job.update(update=ordinal,latest_checkpoint=record['checkpoint'])
    job.setdefault('checkpoint_chunks',{})[str(ordinal)] = sorted(learner.buffer.retained_chunks())
    write(directory/'state.json',job)


def retain(learner, directory, job):
    config = learner.config['retention']
    keep = set(config['checkpoints'])
    keep.update(range(max(0,learner.ordinal-config['recovery_updates']+1),learner.ordinal+1))
    chunks = learner.buffer.retained_chunks()|keep
    for ordinal in keep:
        chunks.update(job.get('checkpoint_chunks',{}).get(str(ordinal),[]))
    # Use the publication ledger, not a recursive filesystem scan per update.
    for name in list(job.get('retained_files',{})):
        from pathlib import Path
        path = Path(name)
        if path.parent.name not in ('chunks','checkpoints'):
            continue
        allowed = chunks if path.parent.name=='chunks' else keep
        if int(path.name.split('.')[0]) not in allowed:
            account_remove(job,path)
            if path.parent.name=='checkpoints':
                job.get('checkpoint_chunks',{}).pop(str(int(path.name.split('.')[0])),None)
    write(directory/'state.json',job)
