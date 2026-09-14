"""Study-only conservative ownership accounting and paired fresh input import."""
from pathlib import Path
import os
from unittest.mock import patch

from declarative_quality_branch import checked,read,require,run_observed,sha,write


def inode(path):
    stat=Path(path).stat()
    return f'{stat.st_dev}:{stat.st_ino}:{stat.st_mtime_ns}',stat.st_size


class Budget:
    """Charge owned file versions; release successful last-link removals only.

    Reconcile at command entry and arm boundaries, never per training update.
    Shared or unobserved deleted versions remain conservatively charged.
    """
    def __init__(self,path,limit=None,excluded=()):
        self.path=Path(path)
        if self.path.exists():self.value=read(self.path)
        else:self.value=dict(schema='declarative-quality-storage-v2',limit=limit,bytes=0,
            excluded=sorted(set(excluded)),versions={},controls={})
        self.excluded=set(self.value['excluded'])
    def observe(self,path,control=False):
        path=Path(path)
        if not path.is_file():return
        require(not path.is_symlink(),f'quality-study.storage: unexpected symlink {path}')
        key,size=inode(path)
        if key in self.excluded:return
        ledger=self.value['controls'] if control else self.value['versions']
        if control:key=str(path)
        old=ledger.get(key,0);ledger[key]=max(old,size)
        self.value['bytes']+=max(0,size-old)
        require(self.value['bytes']<self.value['limit'],'quality-study.storage: 12 GiB conservative added-byte ceiling reached')
    def removing(self,path):
        path=Path(path);key,size=inode(path)
        return key,size,path.stat().st_nlink
    def removed(self,captured):
        key,size,links=captured
        if links==1 and key in self.value['versions']:
            charged=self.value['versions'].pop(key)
            self.value['bytes']-=charged
    def scan(self,folder):
        for path in Path(folder).rglob('*'):
            self.observe(path,control=path.name in ('state.json','console.log','progress.json') or path==self.path)
    def actual(self,folder):
        versions={}
        for path in Path(folder).rglob('*'):
            if not path.is_file():continue
            key,size=inode(path)
            if key not in self.excluded:versions[key]=size
        return dict(bytes=sum(versions.values()),files=len(versions))
    def save(self):write(self.path,self.value)


def fresh_folder(bank,slot,row):
    if row.get('start_curriculum',{}).get('depth',0)!=0:return None
    directory=row.get('input_directory')
    require(directory is None or Path(directory).resolve()==Path(bank).resolve(),
        'quality-study.inputs: unexpected fresh-bank owner')
    path=checked(row['reference'],Path(bank))
    expected=Path(bank).resolve()/f'episode-{int(slot):08d}'
    require(path.parent==expected and path.name=='episode.json','quality-study.inputs: fresh slot path mismatch')
    return expected


def sync_fresh(branch,shared,first_slot,guard):
    launch=read(Path(branch)/'launch.json');owner=Path(branch)/f'seed-{launch["config"]["seeds"][0]}'
    job=read(owner/'state.json');bank=Path(launch['config']['training_input']['directory']).resolve();rows=[]
    for slot,row in job['inputs'].items():
        if int(slot)<first_slot:continue
        folder=fresh_folder(bank,slot,row)
        if folder is None:continue
        guard();files=0
        for path in sorted(folder.rglob('*')):
            if not path.is_file() or path.name.endswith('.lock'):continue
            require(not path.is_symlink(),'quality-study.inputs: symlink in fresh slot')
            target=Path(shared)/path.relative_to(bank);target.parent.mkdir(parents=True,exist_ok=True)
            if target.exists():
                require(inode(path)[0]==inode(target)[0] or sha(path)==sha(target),
                    f'quality-study.inputs: conflicting paired fresh slot {slot}: {path.name}')
            else:os.link(path,target)
            files+=1
        rows.append(dict(slot=int(slot),input_sha256=row['input_sha256'],geometry_sha256=row['geometry_sha256'],files=files))
    return rows


def run_budgeted(branch,environment,budget_path,*,execute=None):
    import declarative_learning_run as runner
    import declarative_learning_runtime as runtime
    budget=Budget(budget_path);budget.scan(Path(budget_path).parent);budget.save()
    launch=read(Path(branch)/'launch.json')
    bank=Path(launch['config']['training_input']['directory']).resolve();seen=set()
    original_remove=runtime.account_remove
    original_account=runtime.account_write;original_register=runner.register_inputs;original_retain=runner.retain
    def account(job,path):
        original_account(job,path);budget.observe(path)
    def remove(job,path):
        captured=budget.removing(path)
        original_remove(job,path)
        budget.removed(captured)
    def register(payload,job):
        original_register(payload,job)
        for before in payload['native_before']:
            if before['policy_actions']!=0:continue
            slot=str(before['metadata']['training_input']['identity']['slot'])
            if slot in seen:continue
            seen.add(slot);folder=fresh_folder(bank,slot,job['inputs'][slot])
            if folder:budget.scan(folder)
    def retain(learner,owner,job):
        original_retain(learner,owner,job)
        budget.observe(owner/'state.json',control=True)
    try:
        with patch.object(runtime,'account_write',account),patch.object(runner,'account_write',account),\
             patch.object(runtime,'account_remove',remove),patch.object(runner,'account_remove',remove),\
             patch.object(runner,'register_inputs',register),patch.object(runner,'retain',retain):
            return (execute or run_observed)(branch,environment)
    finally:
        budget.save()
