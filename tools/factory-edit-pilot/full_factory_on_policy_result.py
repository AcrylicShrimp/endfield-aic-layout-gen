"""Describe immutable completed or arm-budget-ended training without resuming it."""
import argparse
import math
from pathlib import Path

from full_factory_large_learning import read,verify_sources
from full_factory_screen import sha,write


def require(condition,code):
    if not condition:
        raise ValueError(f'on-policy.result-{code}')


def identity(file,expected=None):
    require(file.is_file(),f'missing-file: {file}')
    actual=sha(file)
    require(expected is None or actual==expected,f'file-hash: {file}')
    return actual


def build_result(protocol,report,screen,source_file,protocol_hash):
    """Account for the recorded prefix; independent audits verify its behavior."""
    screen=screen.resolve()
    config=protocol['config']
    require(protocol.get('schema_version')==1 and report.get('schema_version')==1,'schema')
    names=config['initializations']
    planned=config['groups']
    per_group=config['group_episodes']
    schedule=protocol['schedule']
    require(len(schedule)==planned and [s['group'] for s in schedule]==list(range(planned)),'schedule')
    require(len(set(names))==len(names) and names,'initializations')
    complete=source_file.name=='training-report.json'
    failed=None
    if complete:
        require(report['status']=='complete' and report['protocol_sha256']==protocol_hash,'completed-source')
        require(report.get('teacher_labels_used') is False and report.get('future_evaluation_opened') is False,'completed-boundary')
        counts={name:planned for name in names}
        completed_names=names
    else:
        require(source_file.name=='partial.json' and report['status']=='incomplete','partial-source')
        failed=report.get('initialization')
        require(failed in names and report.get('code')=='on-policy.training-failed' and report.get('stage')=='training'
                and report.get('message')==f'on-policy.arm-budget: {failed}','unsupported-termination')
        index=names.index(failed)
        completed_names=names[:index]
        count=report.get('group')
        require(type(count) is int and 0<=count<planned,'failed-group')
        counts={name:planned for name in completed_names}
        counts[failed]=count
    require([a['initialization'] for a in report['arms']]==completed_names,'completed-arm-membership')
    arm_sources={a['initialization']:a for a in report['arms']}
    expected_groups=[(name,g) for name,n in counts.items() for g in range(n)]
    require([(g['initialization'],g['group']) for g in report['groups']]==expected_groups,'group-prefix')
    groups={(g['initialization'],g['group']):g for g in report['groups']}
    applied_episodes=len(expected_groups)*per_group
    unapplied=len(report['rows'])-applied_episodes
    require(unapplied==0 if complete else 0<=unapplied<=per_group,'episode-count')
    expected_rows=[(name,g,i) for name,g in expected_groups for i in range(per_group)]
    if failed is not None:
        expected_rows += [(failed,counts[failed],i) for i in range(unapplied)]
    require([(r['initialization'],r['group'],r['attempt']) for r in report['rows']]==expected_rows,'episode-prefix')
    expected_directories=set()
    grouped={key:[] for key in expected_groups}
    for row in report['rows']:
        name,g,i=row['initialization'],row['group'],row['attempt']
        scheduled=schedule[g]
        require(row['phase']=='training' and row['case']==scheduled['case'] and row['bank']==scheduled['sample_seeds'][i],'episode-schedule')
        require(row['budget_complete'] is True and row['selected']==config['horizon'],'incomplete-episode')
        directory=screen/name/f'group-{g:04d}'/f'episode-{i}'
        require(Path(row['directory']).resolve()==directory,'episode-directory')
        expected_directories.add(directory)
        for file in ['episode.json.gz','best.json.gz','states.json.gz']:
            require((directory/file).is_file(),f'missing-episode-file: {directory/file}')
        if (name,g) in grouped:
            grouped[name,g].append(row)
    actual_directories={d.resolve() for d in screen.glob('*/group-*/episode-*')}
    require(actual_directories==expected_directories,'episode-directory-membership')
    expected_checkpoints=set()
    expected_group_reports=set()
    for key,group in groups.items():
        name,g=key
        require(group['case']==schedule[g]['case'] and group['sample_seeds']==schedule[g]['sample_seeds'],'group-schedule')
        require(group['scores']==[r['best_metrics']['score'] for r in grouped[key]],'group-scores')
        file=screen/name/f'group-{g:04d}'/'checkpoint.pt'
        require(Path(group['checkpoint']).resolve()==file,'group-checkpoint-path')
        identity(file,group['checkpoint_sha256'])
        expected_checkpoints.add(file)
        record=file.with_name('group.json')
        require(record.is_file() and read(record)==group,'group-record')
        expected_group_reports.add(record)
    arms=[]
    for name,n in counts.items():
        initial=screen/name/'initial.pt'
        initial_hash=identity(initial,arm_sources.get(name,{}).get('initial_sha256'))
        expected_checkpoints.add(initial)
        measured=arm_sources.get(name)
        selected_groups=[groups[name,g] for g in range(n)]
        if measured is not None:
            require(measured['groups']==planned and Path(measured['initial']).resolve()==initial,'completed-arm')
            terminal=screen/name/'final.pt'
            require(Path(measured['final']).resolve()==terminal,'final-checkpoint-path')
            terminal_hash=identity(terminal,measured['final_sha256'])
            expected_checkpoints.add(terminal)
            require(type(measured['seconds']) in (float,int) and math.isfinite(measured['seconds']) and measured['seconds']>0,'arm-seconds')
        else:
            terminal=Path(selected_groups[-1]['checkpoint']) if n else initial
            terminal_hash=selected_groups[-1]['checkpoint_sha256'] if n else initial_hash
        nonflat=sum(not g['flat_returns'] for g in selected_groups)
        reward_groups=sum(g['reward_gradient_norm']>0 for g in selected_groups)
        if measured is not None:
            require(measured['nonflat_groups']==nonflat and measured['reward_gradient_groups']==reward_groups,'arm-gradient-counts')
        arms.append(dict(initialization=name,groups=n,planned_groups=planned,nonflat_groups=nonflat,reward_gradient_groups=reward_groups,
            initial=str(initial),initial_sha256=initial_hash,terminal=str(terminal),terminal_sha256=terminal_hash,
            terminal_role='planned_final' if measured is not None else 'last_completed_update' if n else 'initial_no_update',
            seconds=measured['seconds'] if measured is not None else None,
            elapsed_seconds_lower_bound=measured['seconds'] if measured is not None else config['arm_seconds'],
            parameters=measured.get('parameters') if measured is not None else None,
            completed_group_episodes=n*per_group,unapplied_episodes=unapplied if name==failed else 0))
    actual_checkpoints={f.resolve() for pattern in ['*/initial.pt','*/final.pt','*/group-*/checkpoint.pt'] for f in screen.glob(pattern)}
    require(actual_checkpoints==expected_checkpoints,'checkpoint-membership')
    require({f.resolve() for f in screen.glob('*/group-*/group.json')}==expected_group_reports,'group-record-membership')
    return dict(schema_version=1,status='complete' if complete else 'budget_exhausted',training_complete=complete,
        protocol_sha256=protocol_hash,source_report=str(source_file.resolve()),source_report_sha256=identity(source_file),
        analysis_source_sha256={str(Path(__file__).resolve()):sha(Path(__file__))},
        termination=dict(code='on-policy.planned-updates-complete' if complete else 'on-policy.arm-budget',
            initialization=failed,group=None if complete else counts[failed],configured_arm_seconds=config['arm_seconds'],
            message=None if complete else report['message']),
        planned_groups=len(names)*planned,completed_groups=len(expected_groups),
        planned_episodes=len(names)*planned*per_group,completed_group_episodes=applied_episodes,unapplied_episodes=unapplied,
        planned_selected_actions=len(names)*planned*per_group*config['horizon'],sampled_selected_actions=sum(r['selected'] for r in report['rows']),
        gradient_replayed_selected_actions=applied_episodes*config['horizon'],
        unstarted_initializations=names[len(counts):],arms=arms,rows=report['rows'],groups=report['groups'],
        seconds=report.get('seconds') if complete else None,
        scope='Original terminal report preserved; completed optimizer updates and all sampled episodes, including unapplied episodes. Budget-ended arm elapsed time is a lower bound, not an exact measured duration.')


def terminal_source(screen):
    files=[screen/name for name in ['training-report.json','partial.json'] if (screen/name).exists()]
    require(len(files)==1,'terminal-source-count')
    return files[0]


def load_result(screen):
    protocol=read(screen/'protocol.json')
    verify_sources(protocol)
    source=terminal_source(screen)
    result=read(screen/'training-result.json')
    require(result==build_result(protocol,read(source),screen,source,sha(screen/'protocol.json')),'terminal-result-identity')
    return protocol,result


def main(args):
    source=terminal_source(args.screen)
    protocol=read(args.screen/'protocol.json')
    verify_sources(protocol)
    result=build_result(protocol,read(source),args.screen,source,sha(args.screen/'protocol.json'))
    if args.output.exists():
        require(read(args.output)==result,'existing-result-drift')
    else:
        write(args.output,result)
    print({key:result[key] for key in ['status','completed_groups','completed_group_episodes','unapplied_episodes','sampled_selected_actions']},flush=True)


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    for name in ['screen','output']:
        parser.add_argument('--'+name,required=True,type=Path)
    args=parser.parse_args()
    try:
        main(args)
    except Exception as error:
        # Do not overwrite an existing immutable result with a failure report.
        print(dict(status='failed',diagnostics=[dict(code='on-policy.terminal-result-failed',severity='error',stage='terminal-result',
            entity=str(args.screen.resolve()),message=str(error) or type(error).__name__)]),flush=True)
        raise
