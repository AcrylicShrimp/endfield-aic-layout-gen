"""Episode-boundary duration stopping and retained evaluation checkpoint selection."""
from contextlib import contextmanager
import signal
import statistics

from blueprint_ppo_update import require


def validate(config):
    duration=config['duration'];training=config['training']
    require(duration['schema']=='declarative-duration-v1' and duration['seconds']>0 and duration['finish_grace_seconds']>0,
        'declarative.duration: positive duration and finalization allowance required')
    require(len(config['seeds'])==1 and training['streams']==32 and training['chunk_actions']==32 and training['episode_actions']==64,
        'declarative.duration: one learner, batch 1024 and complete 64-action episodes required')
    require(config['evaluation']['interval_updates']>0 and config['evaluation']['interval_updates']%2==0,
        'declarative.duration: evaluate at complete episode boundaries')
    require(config['limits']['arm_seconds']==duration['seconds']+duration['finish_grace_seconds'],
        'declarative.duration: explicit hard finalization deadline required')
    require(config['evaluation']['at']==[training['first_update']], 'declarative.duration: reuse exactly the initial parent evaluation')


def stop_reason(config, ordinal, elapsed, requested=False):
    if ordinal%2:return None
    if requested:return 'requested_stop'
    if elapsed>=config['duration']['seconds'] and ordinal>config['training']['first_update']:return 'duration'
    if ordinal>=config['training']['updates']:return 'update_limit'
    return None


def evaluation_updates(config, endpoint, pause_boundaries=()):
    first=config['training']['first_update']
    require(all(first<=n<=endpoint and n%2==0 for n in pause_boundaries), 'declarative.duration: invalid saved pause evaluation boundary')
    return sorted({*range(first,endpoint+1,config['evaluation']['interval_updates']),*pause_boundaries,endpoint})


def evaluation_due(config, ordinal, stopping=False):
    if not config.get('duration'):return ordinal in config['evaluation']['at']
    return stopping or (ordinal-config['training']['first_update'])%config['evaluation']['interval_updates']==0


def select_best(job, config, evaluation, report_ref):
    for name,products in dict(all=None,**config['strata']).items():
        rows=[r for r in evaluation['rows'] if products is None or r['product'] in products]
        require(rows and all(r['factory_validated'] for r in rows), 'declarative.duration: complete validated selection cohort required')
        score=[statistics.mean(r['total_area_reduction_percent'] for r in rows),
            -statistics.mean(r['best_score'][1] for r in rows),-statistics.mean(r['best_score'][2] for r in rows)]
        old=job.setdefault('best_evaluations',{}).get(name)
        if old is None or score>old['score']:
            job['best_evaluations'][name]=dict(update=evaluation['update'],score=score,report=report_ref,checkpoint=job['latest_checkpoint'])


@contextmanager
def requested_stop():
    requested=[False]
    def receive(*_):requested[0]=True
    old={s:signal.signal(s,receive) for s in (signal.SIGTERM,signal.SIGINT)}
    try:yield lambda:requested[0]
    finally:
        for signum,handler in old.items():signal.signal(signum,handler)
