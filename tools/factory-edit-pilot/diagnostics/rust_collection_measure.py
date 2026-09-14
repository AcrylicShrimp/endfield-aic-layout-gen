"""Bounded native collector measurement using the existing process-tree guard."""
import argparse
import json
import os
from pathlib import Path
import resource
import subprocess
import sys
import time
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from blueprint_execution_resources import ResourceGuard


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--measurement',type=Path,required=True)
    parser.add_argument('command',nargs=argparse.REMAINDER)
    args=parser.parse_args()
    command=args.command[1:] if args.command and args.command[0]=='--' else args.command
    assert command and not args.measurement.exists()
    args.measurement.mkdir(parents=True)
    began=time.monotonic()
    with (args.measurement/'run.log').open('w') as log:
        child=subprocess.Popen(command,stdout=log,stderr=subprocess.STDOUT)
        guard=ResourceGuard(args.measurement,180,40*1024**3,4*1024**3,root_pid=child.pid)
        try:
            while child.poll() is None:
                guard();time.sleep(.5)
        except BaseException:
            child.terminate()
            try:child.wait(timeout=10)
            except subprocess.TimeoutExpired:child.kill();child.wait()
            raise
    usage=resource.getrusage(resource.RUSAGE_CHILDREN)
    result=dict(schema='aic-native-collection-measurement-v1',command=command,exit_code=child.returncode,
        wall_seconds=time.monotonic()-began,maximum_rss_bytes=usage.ru_maxrss*1024,
        user_seconds=usage.ru_utime,system_seconds=usage.ru_stime,guard_last=guard.last,
        measurement_only=True)
    (args.measurement/'summary.json').write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps(result,indent=2))
    sys.exit(child.returncode)


if __name__=='__main__':main()
