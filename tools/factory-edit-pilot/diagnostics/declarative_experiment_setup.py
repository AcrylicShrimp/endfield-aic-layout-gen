"""Isolated diagnostic setup reusing the normal migration and input-store paths."""
from copy import deepcopy
from pathlib import Path
import os

from blueprint_execution_resources import cpu_allocation
from blueprint_ppo_store import checked, sha, write
from blueprint_ppo_update import require
from declarative_execution_migrate import initialize
from declarative_learning_launch import sources, validate


def link_parent_sources(value, output):
    """Reuse already compact, authenticated sources without serializing them again."""
    output=Path(output);output.mkdir()
    job=value['job'];require('replay_sources' in job,'success-arrivals.parent: compact source index required')
    for ref in job['replay_sources'].values():
        source=checked(ref,value['owner']);target=output/ref['path']
        target.parent.mkdir(parents=True,exist_ok=True);os.link(source,target)
    manifest=dict(schema='declarative-compact-parent-v1',status='passed',parent=value['binding'],
        replay_sources=deepcopy(job['replay_sources']),checkpoint_chunks=deepcopy(job['checkpoint_chunks']),
        reused_existing_sources=True)
    write(output/'manifest.json',manifest)
    return manifest


def prepare_baseline(value, output, *, updates, seconds):
    """A bounded normal runner; only duration/endpoint/tracking and locators change."""
    output=Path(output);output.mkdir()
    compact_directory=output/'parent-sources'
    compacted=link_parent_sources(value,compact_directory)
    config=deepcopy(value['launch']['config']);first=value['saved']['update']
    require(0<updates<=128 and updates%2==0 and 0<seconds<=1800,'success-arrivals.bounds: invalid capture budget')
    config['training'].update(first_update=first,updates=first+updates)
    config['evaluation']['at']=[first]
    grace=min(config['duration']['finish_grace_seconds'],seconds/3)
    config['duration'].update(seconds=seconds-grace,finish_grace_seconds=grace)
    config['limits'].update(arm_seconds=seconds,matrix_seconds=seconds)
    config['retention']['training_traces']=[]
    config['tracking']['mode']='disabled'
    validate(config)
    allocation=cpu_allocation(config.get('execution_options',{}).get('workers',8))
    child=output/'baseline';child.mkdir()
    receipt=initialize(value,config,sources(config,value['launch']['source']['native_library']),
        compacted,compact_directory,compact_directory/'manifest.json',child,allocation,diagnostic=True)
    return dict(directory=child,receipt=receipt,launch_sha256=sha(child/'launch.json'))
