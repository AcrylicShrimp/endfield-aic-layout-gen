"""Five-update bounded check of normal quality-study publication and live restore."""
import argparse
from copy import deepcopy
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time

sys.path.insert(0,str(Path(__file__).resolve().parent))
from declarative_quality_branch import BASE,ROOT,checked,digest,endpoint,load,metadata,parent,prepare,read,require,sha,source_binding,write
from blueprint_execution_resources import ResourceGuard


def authenticate(value,source_root):
    source_root=Path(source_root).resolve();source=value['launch']['source']
    for name,expected in source['files_sha256'].items():
        require(sha(source_root/name)==expected,f'quality-study.parent-source: changed {source_root/name}')
    require(sha(source['native_library'])==source['native_sha256'],'quality-study.parent-source: changed native binary')
    require(sha(value['launch']['panel']['path'])==value['launch']['panel']['sha256'],
        'quality-study.parent-source: changed original evaluation panel')
    return dict(source_root=str(source_root),source_sha256=source['code_sha256'])


def run_child(directory,environment,guard):
    directory=Path(directory)
    with (directory/'console.log').open('w') as log:
        process=subprocess.Popen([sys.executable,'-u',str(BASE/'operations/declarative_quality_branch.py'),'run',
            '--branch',str(directory),'--environment',str(environment)],cwd=ROOT,stdout=log,stderr=subprocess.STDOUT,start_new_session=True)
        try:
            while True:
                guard()
                try:code=process.wait(timeout=1.);break
                except subprocess.TimeoutExpired:pass
            require(code==0,f'quality-study.execution: exit {code}; inspect {directory / "console.log"}')
        except BaseException:
            if process.poll() is None:
                process.send_signal(signal.SIGTERM)
                try:process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    os.killpg(process.pid,signal.SIGKILL);process.wait()
            raise
    return read(directory/'state.json')


def verify(parent_path,parent_source,output,environment,cuda_proof):
    output=Path(output).resolve();require(not output.exists(),f'quality-study.output: fresh readiness directory required: {output}')
    value=parent(parent_path);original=authenticate(value,parent_source);limits=value['launch']['config']['limits']
    output.mkdir(parents=True);guard=ResourceGuard(output,600,limits['matrix_rss_bytes'],limits['min_free_bytes'])
    features=metadata(value,guard);write(output/'initial-metadata.json',dict(parent=value['binding'],entries=features))
    remaining=lambda:600-(time.monotonic()-guard.began)
    panel=value['launch']['panel']['path'];branch=output/'uninterrupted'
    proof=prepare(value,branch,profile='score-8',panel=panel,updates=4,seconds=remaining(),features=features,
        retain_live=True,guard=guard)
    run_child(branch,environment,guard)
    completed=endpoint(branch)
    require(completed['saved']['update']==value['saved']['update']+4,'quality-study.readiness: incomplete four-update endpoint')
    live=endpoint(branch,live_job=branch/'live-job.json')
    restored=output/'restored'
    prepare(live,restored,profile='score-8',panel=panel,updates=1,seconds=remaining(),
        initial_quartiles=proof['initial_quartiles'],guard=guard)
    run_child(restored,environment,guard)
    from declarative_quality_closeout import close
    report=close(output,ROOT,cuda_proof,output/'readiness.json',seconds=min(120.,remaining()))
    report.update(seconds=time.monotonic()-guard.began,parent_source=original)
    guard();write(output/'readiness.json',report);return report


def reference_local(path):
    return dict(path=str(path),sha256=sha(path))


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--parent',type=Path,required=True);parser.add_argument('--parent-source',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True);parser.add_argument('--environment',type=Path,required=True)
    parser.add_argument('--cuda-proof',type=Path,required=True)
    args=parser.parse_args()
    try:
        result=verify(args.parent,args.parent_source,args.output,args.environment,args.cuda_proof)
        print(json.dumps({k:result[k] for k in ('status','seconds','executed_updates')}));return 0
    except (ValueError,KeyError,TypeError,OSError,RuntimeError) as error:
        diagnostic=dict(code='quality-study.readiness',severity='error',stage='verify',message=str(error),entity=str(args.parent))
        if args.output.exists():write(args.output/'failure.json',dict(status='failed',diagnostic=diagnostic))
        print(json.dumps(diagnostic),file=sys.stderr);return 2


if __name__=='__main__':raise SystemExit(main())
