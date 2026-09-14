"""Actual-input history lives once in update records; recovery keeps one wave."""
from blueprint_ppo_store import checked,read,reference,write
from blueprint_ppo_update import require

SCHEMA='declarative-input-history-v2'


def initialize(job, directory):
    if job.get('input_history_schema')==SCHEMA:return
    old=job.get('inputs',{})
    if old:
        # Explicit source migrations may start with a legacy in-state prefix.
        # Preserve it once, without rewriting it in every recovery publication.
        path=directory/'input-history-prefix.json'
        write(path,old)
        job['input_history_prefix']=reference(path,directory)
        from declarative_learning_runtime import account_write
        account_write(job,path)
    job.update(schema='declarative-learning-state-v2',input_history_schema=SCHEMA,
        input_count=len(old),last_input_slot=max((int(k) for k in old),default=-1),inputs={})


def register(job, rows):
    if not rows:return
    if job.get('input_history_schema')!=SCHEMA:
        # Existing bounded research drivers expose their in-memory input map.
        # Only the normal runner publishes the v2 recovery/history contract.
        latest=job.setdefault('inputs',{})
        for key,row in rows.items():
            require(key not in latest or latest[key]==row,'declarative changed actual training input slot')
            latest[key]=row
        return
    latest=job['inputs'];last=job['last_input_slot'];new=0
    for key,row in rows.items():
        if int(key)<=last:
            require(latest.get(key)==row,'declarative changed or recycled actual training input slot')
        else:new+=1
    job.update(inputs=rows,input_count=job['input_count']+new,
        last_input_slot=max(last,max(map(int,rows))))


def materialize(job, directory):
    """Explicit reports may inspect history; normal restoration never does."""
    if job.get('input_history_schema')!=SCHEMA:return job['inputs']
    result=read(checked(job['input_history_prefix'],directory)) if job.get('input_history_prefix') else {}
    for ref in job['records']:
        for key,row in read(checked(ref,directory)).get('inputs',{}).items():
            require(key not in result or result[key]==row,'declarative historical input identity conflict')
            result[key]=row
    require(len(result)==job['input_count'],'declarative actual-input history coverage')
    return result
