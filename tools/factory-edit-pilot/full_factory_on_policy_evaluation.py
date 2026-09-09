"""Freeze prospective whole-factory evaluation after audited on-policy training."""
import argparse
from pathlib import Path
import time

import torch

from full_factory_curriculum import source_manifest
from full_factory_large_learning import read,verify_sources
from full_factory_on_policy import on_policy_rollout,parameter_hash
from full_factory_on_policy_learning import config_check
from full_factory_on_policy_result import load_result
from full_factory_policy import FullFactoryPolicy,SCHEMA
from full_factory_screen import sha,write


def freeze(args):
    p,r=load_result(args.training)
    a=read(args.training/'training-audit.json')
    if a['status']!='passed' or a['report_sha256']!=sha(args.training/'training-result.json') or r['unstarted_initializations'] or [arm['initialization'] for arm in r['arms']]!=p['config']['initializations']:
        raise ValueError('on-policy.evaluation-training-audit')
    verify_sources(p)
    admission,inputs=read(args.admission/'report.json'),read(args.inputs)
    accepted=[row for row in admission['rows'] if row['status']=='accepted' and row['factory_validated']]
    if admission['status']!='measured' or admission['attempted']!=len(admission['rows']) or admission['accepted']!=len(accepted) or [row['id'] for row in accepted]!=[case['id'] for case in inputs['rows']]:
        raise ValueError('on-policy.future-admission-membership')
    if not any(Path(s['path']).resolve()==Path(p['future_evaluation']).resolve() and s['sha256']==sha(p['future_evaluation']) for s in admission['provenance']):
        raise ValueError('on-policy.future-config-identity')
    if (args.training/'training-result.json').stat().st_mtime_ns >= (args.admission/'report.json').stat().st_mtime_ns:
        raise ValueError('on-policy.future-opened-before-training-freeze')
    if {k:sha(f) for k,f in read(inputs['catalogs']).items()}!={k:sha(f) for k,f in read(p['catalogs']).items()}:
        raise ValueError('on-policy.future-catalog-drift')
    cases=list(p['cases'])+[dict(case,family=row['family'],split='development',transfer_group='prospective-future',
        internal_pipe_requirements=row['internal_pipe_requirements'],topology=row['topology']) for case,row in zip(inputs['rows'],accepted)]
    if len({case['id'] for case in cases})!=len(cases):
        raise ValueError('on-policy.duplicate-evaluation-case')
    checkpoints={}
    for arm in r['arms']:
        for phase in ['initial','terminal']:
            if sha(arm[phase])!=arm[f'{phase}_sha256']:
                raise ValueError('on-policy.evaluation-checkpoint-hash')
            checkpoints[f'{arm["initialization"]}-{phase}']=dict(path=arm[phase],sha256=arm[f'{phase}_sha256'],
                initialization=arm['initialization'],phase=phase,protocol_sha256=sha(args.training/'protocol.json'),
                completed_updates=0 if phase=='initial' else arm['groups'],terminal_role=arm['terminal_role'] if phase=='terminal' else 'initial')
    sources=dict(p['sources'])
    sources.update({str(Path(s['path']).resolve()):s['sha256'] for s in admission['provenance']+inputs['provenance']})
    files=[args.training/n for n in ['protocol.json','training-result.json','training-audit.json']]+[Path(r['source_report'])]
    files += [args.admission/'report.json',args.inputs,Path(inputs['catalogs'])]+[Path(c['path']) for c in checkpoints.values()]
    for case in cases:
        for key,name in [('initial','initial.json'),('original','original-request.json'),('request','request.json')]:
            if sha(case[key])!=case['files_sha256'][name]:
                raise ValueError('on-policy.evaluation-case-hash')
            files.append(Path(case[key]))
    sources.update({str(f.resolve()):sha(f) for f in files})
    protocol=dict(schema_version=1,experiment='on-policy-frozen-prospective-evaluation',config=p['config'],cases=cases,checkpoints=checkpoints,
        methods=[*checkpoints,'geometry'],training=str(args.training.resolve()),admission=str(args.admission.resolve()),inputs=str(args.inputs.resolve()),
        schema=SCHEMA,depth=p['depth'],width=p['width'],catalogs=p['catalogs'],native_library=p['native_library'],
        sources=sources,source_sha256=source_manifest(),training_performed=False,training_result_status=r['status'],
        training_completed_updates={arm['initialization']:arm['groups'] for arm in r['arms']})
    verify_sources(protocol)
    args.output.mkdir()
    write(args.output/'protocol.json',protocol)
    print(dict(status='frozen',cases=len(cases),episodes=len(cases)*len(protocol['methods'])*len(p['config']['evaluation_banks'])),flush=True)


def evaluate(args):
    began=time.perf_counter()
    torch.set_num_threads(1)
    torch.use_deterministic_algorithms(True)
    p=read(args.screen/'protocol.json')
    config_check(p['config'])
    verify_sources(p)
    if any((args.screen/n).exists() for n in ['report.json','progress.json','partial.json']):
        raise ValueError('on-policy.existing-evaluation: use a fresh screen')
    rows=[]
    active_method=active_case=None
    try:
        for method in p['methods']:
            active_method=method
            model=None
            if method!='geometry':
                record=p['checkpoints'][method]
                checkpoint=torch.load(record['path'],map_location='cpu',weights_only=False)
                if sha(record['path'])!=record['sha256'] or checkpoint['protocol_sha256']!=record['protocol_sha256'] or checkpoint['schema']!=SCHEMA:
                    raise ValueError('on-policy.evaluation-policy-identity')
                model=FullFactoryPolicy(p['depth'],p['width'])
                model.load_state_dict(checkpoint['policy'],strict=True)
                model.eval()
                before=parameter_hash(model)
            for case in p['cases']:
                active_case=case['id']
                for bank in p['config']['evaluation_banks']:
                    if time.perf_counter()-began>=p['config']['evaluation_seconds']:
                        raise TimeoutError('on-policy.evaluation-study-budget')
                    row,_=on_policy_rollout(p,case,model,bank,args.screen/f'{method}--{case["id"]}--{bank}',save_states=False,geometry=method=='geometry')
                    row.update(method=method,phase='evaluation')
                    rows.append(row)
                    write(args.screen/'progress.json',dict(status='incomplete',rows=rows))
                print(dict(method=method,case=case['id'],episodes=len(rows),score=rows[-1]['best_metrics']['score'],seconds=time.perf_counter()-began),flush=True)
            if model is not None and parameter_hash(model)!=before:
                raise ValueError('on-policy.evaluation-mutated-policy')
        verify_sources(p)
        write(args.screen/'report.json',dict(schema_version=1,status='complete',protocol_sha256=sha(args.screen/'protocol.json'),
            seconds=time.perf_counter()-began,policy_immutable=True,training_performed=False,rows=rows))
    except BaseException as error:
        write(args.screen/'partial.json',dict(schema_version=1,status='incomplete',code='on-policy.evaluation-failed',severity='error',stage='evaluation',
            method=active_method,case=active_case,message=str(error),rows=rows))
        raise


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    sub=parser.add_subparsers(dest='command',required=True)
    prepare=sub.add_parser('freeze')
    for name in ['training','admission','inputs','output']:
        prepare.add_argument('--'+name,required=True,type=Path)
    sub.add_parser('evaluate').add_argument('--screen',required=True,type=Path)
    args=parser.parse_args()
    (freeze if args.command=='freeze' else evaluate)(args)
