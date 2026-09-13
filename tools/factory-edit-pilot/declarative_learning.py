"""Prepare, verify, train, resume and report the explicit declarative learner."""
import argparse
from pathlib import Path
import json

from blueprint_ppo_store import read
from declarative_learning_launch import panel, prepare, validate


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('command',choices=('panel','verify','prepare','run','report'))
    parser.add_argument('--config',type=Path,default=Path(__file__).with_name('declarative-curriculum-short.json'))
    parser.add_argument('--native-library',type=Path)
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--panel',type=Path)
    parser.add_argument('--verification',type=Path)
    parser.add_argument('--parent',type=Path)
    parser.add_argument('--resume',action='store_true',help='Resume a bounded verification from its published checkpoint')
    parser.add_argument('--environment',type=Path,default=Path('.env'))
    args=parser.parse_args()
    config=read(args.config)
    validate(config)
    if args.command=='panel':
        if args.native_library is None:parser.error('panel requires --native-library')
        result=panel(config,args.native_library,args.output)
    elif args.command=='verify':
        if args.native_library is None or args.panel is None:parser.error('verify requires --native-library and --panel')
        from declarative_learning_verify_curriculum import verify
        result=verify(config,args.native_library,args.panel,args.output,args.parent,args.resume)
    elif args.command=='prepare':
        if args.native_library is None or args.panel is None or args.verification is None:
            parser.error('prepare requires --native-library, --panel and --verification')
        result=prepare(config,args.native_library,args.output,args.panel,args.verification,args.parent)
    elif args.command=='run':
        from declarative_learning_run import run
        result=run(args.output,args.environment)
    else:
        from declarative_learning_report import report
        result=report(args.output)
    print(json.dumps({k:v for k,v in result.items() if k in ('status','seconds','output','forecast_seconds','continuity_closed')},indent=2))


if __name__=='__main__':
    main()
