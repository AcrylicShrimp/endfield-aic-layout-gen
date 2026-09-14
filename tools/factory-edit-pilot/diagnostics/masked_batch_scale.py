"""Short cold batch scaling with canonical actual inputs and normal persistence."""
import argparse
import datetime
from pathlib import Path
import statistics
import subprocess
import sys

BASE=Path(__file__).resolve().parents[1]
sys.path[:0]=[str(BASE),str(BASE/'operations')]
from blueprint_ppo_store import read,write
from blueprint_ppo_update import require
from masked_long_training import configuration,prepare


def setup(output,panel,reservation,library):
    require(not output.exists(),'masked-batch.output: fresh matrix directory required')
    output.mkdir(parents=True);jobs=[]
    for streams in (32,64,128,256):
        config=configuration(read(reservation),probe=True)
        config['purpose']='masked-batch-scale-readiness'
        config['training'].update(streams=streams,updates=4)
        config['ppo']['minibatch_size']=streams*2
        config['training_input'].update(directory=str(output/'actual-inputs'),canonical_streams=1)
        config['retention']['checkpoints']=[0,4]
        arm=output/f'streams-{streams}'
        prepare(arm,panel,read(reservation),library,probe=True,probe_config=config)
        jobs.append(dict(streams=streams,minibatch_size=streams*2,path=str(arm)))
    result=dict(schema='masked-batch-scale-v1',status='ready',jobs=jobs,
        metric='New environment transitions per minute, excluding replay reuse.',
        warmup_updates=2,measured_updates=2,physical_workers=16,
        continuity=['same normal runner, persistent workers and shared-memory transport',
            'same masked PPO, retention, curriculum, native budgets and per-update recovery',
            'canonical input store reuses identical actual factories at overlapping global slots',
            'cold initialization per arm; no trained state imports; no bulk traces'])
    write(output/'matrix.json',result);return result


def summarize(arm,measurement):
    launch=read(arm/'launch.json');job=read(arm/'seed-149/state.json')
    require(job['status']=='complete' and job['update']==4,'masked-batch.incomplete: complete two-wave arm required')
    owner=arm/'seed-149';files=[owner/r['path'] for r in job['records']]
    records=[read(p) for p in files];begin,end=files[1].stat().st_mtime,files[3].stat().st_mtime
    cpu=read(measurement/'cpu.json');cores=[r['cpu_cores'] for r in cpu['rows'] if begin<=r['unix']<=end]
    gpu=[];memory=[]
    for line in (measurement/'gpu.csv').read_text().splitlines():
        stamp,*values=line.split(',')
        when=datetime.datetime.strptime(stamp.strip(),'%Y/%m/%d %H:%M:%S.%f').timestamp()
        if begin<=when<=end:gpu.append(float(values[0]));memory.append(float(values[2]))
    samples=sum(r['actions'] for r in records[2:]);elapsed=end-begin
    evaluation=read(owner/job['evaluation'][-1]['path'])
    require(all(r['factory_validated'] for r in evaluation['rows']),'masked-batch.validity: invalid final evaluation')
    return dict(status='complete',streams=launch['config']['training']['streams'],minibatch_size=launch['config']['ppo']['minibatch_size'],
        measured_new_samples=samples,measurement_seconds=elapsed,new_samples_per_minute=samples/elapsed*60,
        seconds_per_update=elapsed/2,cpu_cores=statistics.mean(cores),cpu_percent=statistics.mean(cores)/cpu['physical_cores']*100,
        gpu_percent=statistics.mean(gpu),gpu_peak_mib=max(memory),peak_rss_bytes=job['resource']['peak_rss'],
        all_in_new_samples_per_minute=sum(r['actions'] for r in records)/job['seconds']*60,
        job_seconds=job['seconds'],retained_bytes=job['retained_bytes'],
        rows=[dict(update=r['update'],collection_seconds=r['collection_seconds'],publication_seconds=r['publication_seconds'],
            optimization_seconds=r['ppo']['execution']['update_seconds'],preparation_seconds=r['ppo']['execution']['prepare_worker_seconds'],
            actor_epochs=r['ppo']['actor_epochs'],mean_root_candidates=r['mean_root_candidates'],
            eligible=r['buffer']['eligible']) for r in records],
        cpu_samples=len(cores),gpu_samples=len(gpu),all_final_factories_valid=True)


def run(output,environment):
    matrix=read(output/'matrix.json');results=[]
    for job in matrix['jobs']:
        arm=Path(job['path']);measurement=arm.with_name(arm.name+'-telemetry')
        print(dict(stage='batch',streams=job['streams'],status='starting'),flush=True)
        result=subprocess.run([sys.executable,str(Path(__file__).with_name('masked_cold_profile.py')),
            '--output',str(arm),'--measurement',str(measurement),'--environment',str(environment)])
        if result.returncode:
            results.append(dict(status='failed',streams=job['streams'],log=str(measurement/'run.log')))
            # Larger logical batches cannot fix a resource failure at this size.
            write(output/'results.json',dict(status='partial',rows=results));break
        row=summarize(arm,measurement);results.append(row)
        write(output/'results.json',dict(status='complete' if len(results)==len(matrix['jobs']) else 'running',rows=results))
        print({k:v for k,v in row.items() if k not in ('rows',)},flush=True)
    return results


def compile_pair(output,environment):
    rows=read(output/'results.json')['rows']
    chosen=max((r for r in rows if r['status']=='complete'),key=lambda r:r['new_samples_per_minute'])
    source=read(output/f'streams-{chosen["streams"]}/launch.json');config=source['config']
    # Complete the small common actual-input prefix before either timed arm.
    # This removes input-cache asymmetry, without recording any trajectories.
    from declarative_learning_environment import Runtime
    inputs=Runtime(config,source['source']['native_library']).inputs
    for slot in range(chosen['streams']*2):inputs.acquire(slot%chosen['streams'],slot//chosen['streams'])
    results=[]
    for mode in ('eager','compiled'):
        arm=output/f'paired-{mode}';measurement=output/f'paired-{mode}-telemetry'
        prepare(arm,Path(source['panel']['path']),config['training_input']['geometry_reservation'],
            Path(source['source']['native_library']),probe=True,probe_config=config)
        command=[sys.executable,str(Path(__file__).with_name('masked_cold_profile.py')),
            '--output',str(arm),'--measurement',str(measurement),'--environment',str(environment)]
        if mode=='compiled':command.append('--compile-model')
        print(dict(stage='compilation-pair',mode=mode,streams=chosen['streams'],status='starting'),flush=True)
        process=subprocess.run(command)
        row=summarize(arm,measurement) if process.returncode==0 else dict(status='failed',log=str(measurement/'run.log'))
        row['mode']=mode;results.append(row)
        write(output/'compilation-pair.json',dict(status='complete' if len(results)==2 else 'running',
            input_prefix_frozen=chosen['streams']*2,rows=results))
        print({k:v for k,v in row.items() if k!='rows'},flush=True)
    return results


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('command',choices=('prepare','run','compile-pair'))
    p.add_argument('--output',type=Path,required=True);p.add_argument('--panel',type=Path)
    p.add_argument('--reservation',type=Path);p.add_argument('--native-library',type=Path);p.add_argument('--environment',type=Path)
    a=p.parse_args()
    if a.command=='prepare':setup(a.output.resolve(),a.panel.resolve(),a.reservation.resolve(),a.native_library.resolve())
    elif a.command=='run':run(a.output.resolve(),a.environment.resolve())
    else:compile_pair(a.output.resolve(),a.environment.resolve())
