"""Independently inspect every retained optimizer checkpoint and its policy identity."""
import argparse
from pathlib import Path

import torch

from full_factory_on_policy import parameter_hash
from full_factory_on_policy_result import load_result
from full_factory_policy import FullFactoryPolicy,SCHEMA
from full_factory_screen import sha,write


def audit(args,context):
    torch.set_num_threads(1)
    p,r=load_result(args.screen)
    c=p['config']
    rows=[]
    parameter_count=None
    for arm in r['arms']:
        name=arm['initialization']
        context['entity']=name
        torch.manual_seed(c['seed'])
        model=FullFactoryPolicy(p['depth'],p['width'])
        if name=='imitation-warm':
            model.load_state_dict(torch.load(p['warm_checkpoint'],map_location='cpu',weights_only=False)['policy'],strict=True)
        expected_initial=parameter_hash(model)
        group_rows=[g for g in r['groups'] if g['initialization']==name]
        assert len(group_rows)==arm['groups']<=c['groups']
        count=sum(v.numel() for v in model.parameters())
        assert arm['parameters'] is None or arm['parameters']==count
        assert parameter_count is None or parameter_count==count
        parameter_count=count
        files=[(Path(arm['initial']),0,arm['initial_sha256'])]+[(Path(g['checkpoint']),g['group']+1,g['checkpoint_sha256']) for g in group_rows]
        if Path(arm['terminal'])!=files[-1][0]:
            assert arm['terminal_role']=='planned_final' and arm['groups']==c['groups']
            files.append((Path(arm['terminal']),arm['groups'],arm['terminal_sha256']))
        else:
            assert arm['terminal_sha256']==files[-1][2]
        previous=expected_initial
        for index,(file,group,file_hash) in enumerate(files):
            context['entity']=str(file.resolve())
            assert sha(file)==file_hash
            checkpoint=torch.load(file,map_location='cpu',weights_only=False)
            assert checkpoint['schema']==SCHEMA and checkpoint['protocol_sha256']==sha(args.screen/'protocol.json')
            assert checkpoint['initialization']==name and checkpoint['seed']==c['seed'] and checkpoint['next_group']==group
            assert (checkpoint['depth'],checkpoint['width'])==(p['depth'],p['width'])
            model.load_state_dict(checkpoint['policy'],strict=True)
            assert all(torch.isfinite(v).all() for v in model.state_dict().values())
            actual=parameter_hash(model)
            assert actual==checkpoint['policy_sha256']
            if index==0:
                assert actual==expected_initial
            elif index<=len(group_rows):
                row=group_rows[index-1]
                assert row['group']==index-1 and row['policy_before_sha256']==previous and row['behavior_policy_sha256']==previous
                assert row['policy_after_sha256']==actual and row['parameter_changed']==(previous!=actual)
            else:
                assert actual==previous
            previous=actual
            optimizer=checkpoint['optimizer']
            assert len(optimizer['param_groups'])==1 and optimizer['param_groups'][0]['lr']==c['learning_rate']
            parameters=list(model.parameters())
            assert optimizer['param_groups'][0]['params']==list(range(len(parameters)))
            assert len(optimizer['state'])==(len(parameters) if group else 0)
            for i,state in optimizer['state'].items():
                assert int(state['step'])==group
                for key in ['exp_avg','exp_avg_sq']:
                    assert state[key].shape==parameters[i].shape and torch.isfinite(state[key]).all()
            assert checkpoint['rng'].dtype==torch.uint8 and checkpoint['rng'].shape==torch.get_rng_state().shape
            rows.append(dict(initialization=name,path=str(file.resolve()),sha256=file_hash,group=group,policy_sha256=actual))
    write(args.output,dict(schema_version=1,status='passed',report_sha256=sha(args.screen/'training-result.json'),
        source_report_sha256=r['source_report_sha256'],training_complete=r['training_complete'],parameters=parameter_count,rows=rows,
        scope='Every unique model/Adam/RNG checkpoint, strict architecture and initialization, sequential policy hashes, actual group counts and optimizer steps; terminal aliases do not add updates. Not an independent recomputation of every gradient.'))
    print(dict(status='passed',checkpoints=len(rows)),flush=True)


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    for name in ['screen','output']:parser.add_argument('--'+name,required=True,type=Path)
    args=parser.parse_args()
    context=dict(entity=str(args.screen.resolve()))
    try:
        audit(args,context)
    except Exception as error:
        write(args.output,dict(schema_version=1,status='failed',diagnostics=[dict(
            code='on-policy.checkpoint-audit-failed',severity='error',stage='checkpoint-audit',
            entity=context['entity'],message=str(error) or type(error).__name__)]))
        raise
