"""Bounded paired local probe of validated randomized setup geometry."""
import argparse
from collections import Counter
from itertools import combinations
import json
from pathlib import Path
import random
import subprocess
import sys
import time

sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from blueprint_ppo_store import ROOT, read, reference, write
from blueprint_ppo_update import digest, require
from native_factory_runtime import FactoryRuntime
from native_factory_admission import accept_case, admit
from random_factory_generation import draw, geometry_identity, resolve_spec


def spatial(witness, original):
    placements=sorted(witness['placements'],key=lambda p:p['instance'])
    parents={p['instance']:p for p in original['placements']}
    pairs=list(combinations(placements,2))
    def offset(a,b):return (a['x']-b['x'],a['y']-b['y'])
    difference=[sum(abs(x-y) for x,y in zip(offset(a,b),offset(parents[a['instance']],parents[b['instance']]),strict=True)) for a,b in pairs]
    low_x=min(p['x'] for p in placements);low_y=min(p['y'] for p in placements)
    original_x=min(p['x'] for p in parents.values());original_y=min(p['y'] for p in parents.values())
    return dict(facilities=len(placements),geometry_sha256=geometry_identity(witness),
        geometry_changed=geometry_identity(witness)!=geometry_identity(original),
        changed_relative_origins=sum((p['x']-low_x,p['y']-low_y)!=(parents[p['instance']]['x']-original_x,parents[p['instance']]['y']-original_y) for p in placements),
        changed_rotations=sum(p['rotation']!=parents[p['instance']]['rotation'] for p in placements),
        mean_pair_offset_change_tiles=sum(difference)/len(difference) if difference else 0.,
        axis_aligned_pairs_fraction=sum(a['x']==b['x'] or a['y']==b['y'] for a,b in pairs)/len(pairs) if pairs else None,
        distinct_x_origins=len({p['x'] for p in placements}),distinct_y_origins=len({p['y'] for p in placements}))


def arm(task_path):
    task=read(task_path);directory=Path(task['directory']);spec=task['spec'];began=time.monotonic()
    runtime=FactoryRuntime(task['library'],task['catalogs'])
    initial=read(task['case']['initial']);case=task['case'];setting=task['arm'];chosen=task['draw']
    steps=chosen['setup_steps'] if setting['steps']=='original' else setting['steps']
    env=runtime.open_episode(case,steps,task['setup_seconds'],spec['selection']['route_work'],vocabulary=setting['vocabulary'])
    rng=random.Random(chosen['setup_seed']);actions=[];status='complete';diagnostics=[]
    try:
        for _ in range(steps):
            view=env.candidates()
            kinds=sorted({runtime.kinds[k] for k,ok in zip(view['kind_ids'],view['allowed']) if ok and runtime.kinds[k] in spec['generation']['setup_kinds']})
            require(kinds, 'generator-diversity: no allowed configured setup kind')
            kind=rng.choice(kinds)
            index=rng.choice([i for i,(k,ok) in enumerate(zip(view['kind_ids'],view['allowed'])) if ok and runtime.kinds[k]==kind])
            result=env.step(view['handle'],index);actions.append(dict(kind=kind,status=result['status']))
            if result['status']=='late':raise TimeoutError('native setup deadline')
    except (ValueError,RuntimeError,TimeoutError) as error:
        status='failed';diagnostics.append(dict(code='generator-diversity.setup-failed',severity='error',stage='setup',entity=str(directory),message=str(error)))
    summary=env.summary();write(directory/'setup.json',summary)
    current=json.loads(env.export_layout('current'));write(directory/'current.json',current)
    _,check=accept_case(runtime,read(case['request']),read(case['original']),current,'diversity-probe',directory/'admitted',5.,spec['selection']['route_work'])
    normalized=json.loads(check.export_layout('current'))
    require(digest(normalized)==digest(current), 'generator-diversity actual current start changed during admission')
    record=dict(status=status,arm=setting['name'],steps_requested=steps,steps_completed=len(actions),
        factory_validated=True,current_is_actual_start=True,actions=actions,seconds=time.monotonic()-began,
        metrics=check.summary()['initial_metrics'],spatial=spatial(current,initial),diagnostics=diagnostics,
        current=reference(directory/'current.json',directory))
    write(directory/'result.json',record);runtime.close()
    print(dict(arm=setting['name'],status=status,seconds=round(record['seconds'],3)))


def run(config_path, library, output):
    config=read(config_path);require(config['schema']=='generator-spatial-diversity-probe-v1','generator-diversity config schema')
    output=Path(output).resolve();require(not output.exists(),f'generator-diversity needs fresh output: {output}');output.mkdir(parents=True)
    spec=resolve_spec(ROOT/config['generation_spec']);spec['native_library']=str(Path(library).resolve())
    catalogs=output/'catalogs.json';write(catalogs,spec['catalogs']);runtime=FactoryRuntime(library,catalogs)
    began=time.monotonic();deadline=began+config['seconds']
    def guard():require(time.monotonic()<deadline,'generator-diversity total diagnostic deadline')
    selected={}
    for seed in range(config['seed_base'],config['seed_base']+config['seed_search']):
        chosen=draw(spec,seed);selected.setdefault(chosen['product'],dict(seed=seed,draw=chosen))
        if len(selected)==len(spec['products']):break
    require(len(selected)==len(spec['products']),'generator-diversity seed interval lacks a product')
    write(output/'selection.json',selected);rows=[]
    for product,selection in selected.items():
        directory=output/product;directory.mkdir();chosen=selection['draw']
        case,attempt=admit(runtime,spec,chosen,directory/'construction',guard)
        row=dict(product=product,**selection,admission=attempt,arms=[])
        if case is not None:
            witness=read(case['initial']);row['constructor']=spatial(witness,witness);row['case']=case
            for setting in config['arms']:
                folder=directory/setting['name'];folder.mkdir()
                task=dict(directory=str(folder),library=str(Path(library).resolve()),catalogs=str(catalogs),
                    spec=spec,case=case,draw=chosen,arm=setting,setup_seconds=config['setup_seconds'])
                task_path=folder/'task.json';write(task_path,task)
                try:
                    guard()
                    result=subprocess.run([sys.executable,str(Path(__file__).resolve()),'arm','--task',str(task_path)],
                        capture_output=True,text=True,timeout=min(config['setup_seconds']+15,deadline-time.monotonic()))
                    (folder/'worker.log').write_text(result.stdout+result.stderr)
                    require(result.returncode==0,f'generator-diversity worker exit {result.returncode}: {folder}')
                    measured=read(folder/'result.json')
                except (subprocess.TimeoutExpired,ValueError) as error:
                    measured=dict(arm=setting['name'],status='failed',factory_validated=False,
                        diagnostic=dict(code='generator-diversity.worker-failed',severity='error',stage='setup',entity=str(folder),message=str(error)))
                    write(folder/'failure.json',measured)
                row['arms'].append(measured)
                print(dict(product=product,arm=setting['name'],status=measured['status'],spatial=measured.get('spatial')),flush=True)
        rows.append(row);write(output/'progress.json',rows)
    result=dict(schema=config['schema'],status='complete',seconds=time.monotonic()-began,rows=rows,
        config=reference(config_path,Path(config_path).parent),native=runtime.build,
        interpretation='Four paired fresh requests. Valid current exports, not score-selected bests; setup sampling is not uniform geometry sampling.')
    write(output/'report.json',result);runtime.close();return result


def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('command',choices=('run','arm'))
    parser.add_argument('--config',type=Path,default=Path(__file__).with_name('generator-spatial-diversity.json'))
    parser.add_argument('--native-library',type=Path);parser.add_argument('--output',type=Path);parser.add_argument('--task',type=Path)
    args=parser.parse_args()
    if args.command=='arm':arm(args.task)
    else:
        if args.native_library is None or args.output is None:parser.error('run requires --native-library and --output')
        result=run(args.config,args.native_library,args.output);print(dict(status=result['status'],seconds=result['seconds']))


if __name__=='__main__':main()
