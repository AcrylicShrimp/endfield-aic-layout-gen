"""Extend a paused learner's time budget without changing its training contract."""
import argparse
from copy import deepcopy
from datetime import datetime,timezone
from pathlib import Path
import os
import sys

sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from blueprint_duration_store import exclusive_run
from blueprint_ppo_store import checked,load,read,reference,save,sha,write
from blueprint_ppo_update import require
from declarative_learning_launch import validate
from declarative_learning_runtime import CHECKPOINT,account_write


def deadline(value):
    result=datetime.fromisoformat(value)
    require(result.tzinfo is not None and result.utcoffset() is not None,
            'declarative.extend: --until requires an explicit UTC offset')
    return result.astimezone(timezone.utc)


def extend(output, until, *, now=None):
    output=Path(output).resolve();now=now or datetime.now(timezone.utc)
    require(until>now,'declarative.extend: --until must be in the future')
    with exclusive_run(output):
        old_launch=read(output/'launch.json');old_matrix=read(output/'state.json')
        require(old_matrix['status']=='paused','declarative.extend: pause the learner at an episode boundary first')
        require(old_matrix['launch_sha256']==sha(output/'launch.json'),'declarative.extend: changed launch identity')
        config=deepcopy(old_launch['config']);validate(config)
        require(len(config['seeds'])==1 and bool(config.get('duration')),'declarative.extend: one duration learner required')
        seed=config['seeds'][0];owner=output/f'seed-{seed}';old_job=read(owner/'state.json')
        require(old_job['status']=='paused' and old_job['stop_reason']=='requested_stop',
                'declarative.extend: durable requested pause required')
        saved=load(checked(old_job['latest_checkpoint'],owner))
        require(saved['schema']==CHECKPOINT and saved['protocol_sha256']==old_matrix['launch_sha256'] and
                saved['seed']==seed and saved['update']==old_job['update'],'declarative.extend: checkpoint/launch boundary mismatch')
        require(len(saved['streams'])==config['training']['streams'] and not saved['buffer']['active'] and
                all(s['native'] is None and s['next_start'] is not None for s in saved['streams']),
                'declarative.extend: unfinished episode at the selected pause')
        remaining=(until-now).total_seconds();new_seconds=old_job['seconds']+remaining
        extra=new_seconds-config['duration']['seconds']
        require(extra>0,'declarative.extend: --until does not extend the existing budget')
        config['duration']['seconds']=new_seconds
        config['limits']['arm_seconds']+=extra;config['limits']['matrix_seconds']+=extra
        validate(config)
        archive=output/'launch-history'/f'{old_matrix["launch_sha256"]}.json';archive.parent.mkdir(exist_ok=True)
        if not archive.exists():os.link(output/'launch.json',archive)
        require(sha(archive)==old_matrix['launch_sha256'],'declarative.extend: archived launch identity differs')
        amendment=dict(schema='declarative-duration-amendment-v1',requested_until_utc=until.isoformat(),
            prepared_utc=now.isoformat(),previous_launch=reference(archive,output),update=old_job['update'],
            elapsed_seconds=old_job['seconds'],previous_duration_seconds=old_launch['config']['duration']['seconds'],
            duration_seconds=new_seconds,remaining_seconds=remaining,
            scope='Time limits only; reuse existing execution verification. Finish the active wave and final evaluation at the time boundary.')
        launch=deepcopy(old_launch);launch['config']=config;launch['forecast_seconds']=new_seconds
        launch.setdefault('duration_amendments',[]).append(amendment)
        staged=output/'launch-next.json';write(staged,launch);identity=sha(staged)
        checkpoint=owner/f'initial/deadline-{until.strftime("%Y%m%dT%H%M%SZ")}-{saved["update"]:04d}.pt.gz'
        require(not checkpoint.exists(),f'declarative.extend: checkpoint already exists: {checkpoint}')
        saved['protocol_sha256']=identity;save(checkpoint,saved)
        job=deepcopy(old_job);account_write(job,checkpoint);job['latest_checkpoint']=reference(checkpoint,owner)
        matrix=deepcopy(old_matrix);matrix['launch_sha256']=identity
        # The existing run lock protects this small configuration publication.
        # Historical checkpoints keep their unchanged, archived launch binding.
        try:
            staged.replace(output/'launch.json');write(owner/'state.json',job)
            matrix['jobs'][str(seed)]['state']=reference(owner/'state.json',output)
            write(output/'state.json',matrix)
        except BaseException:
            write(output/'launch.json',old_launch);write(owner/'state.json',old_job);write(output/'state.json',old_matrix)
            raise
        return dict(status='extended',output=str(output),requested_until_utc=until.isoformat(),
            remaining_seconds=remaining,update=job['update'],launch_sha256=identity,checkpoint=job['latest_checkpoint'])


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--output',type=Path,required=True);p.add_argument('--until',type=deadline,required=True)
    a=p.parse_args();print(extend(a.output,a.until))


if __name__=='__main__':main()
