"""Verify input-screen evidence, replay declared samples and summarize paired learning."""
import argparse
from collections import Counter
from pathlib import Path
import time

import torch

from full_factory_environment import FactoryRuntime, FactoryEpisode, digest
from full_factory_input_contracts import (
    SCHEMA, InputContractPolicy, observation, observation_hash, selection_mask,
    selected_action_token, history_record,
)
from full_factory_input_screen import write, config_check
from full_factory_large_learning import read, unpack, verify_sources
from full_factory_on_policy import parameter_hash, probability_hash, relative_returns
from full_factory_screen import sha
from full_factory_selection import SelectionMemory, distribution, physical_key


def require(condition, detail):
    if not condition:
        raise ValueError(f'input-summary.identity: {detail}')


def verify_job(screen, protocol, run_row):
    path = Path(run_row['report'])
    require(run_row['status'] == 'complete' and sha(path) == run_row['report_sha256'], str(path))
    report = read(path)
    config = protocol['config']
    require(report['status'] == 'complete' and report['protocol_sha256'] == sha(screen / 'protocol.json'), 'job protocol')
    require(report['job'] == run_row['job'] and report['contract'] == protocol['contracts'][report['job']['candidate']], 'job contract')
    require([g['group'] for g in report['groups']] == list(range(config['groups'])), 'complete groups')
    require([c['next_group'] for c in report['checkpoints']] == list(range(config['groups'] + 1)), 'complete checkpoints')
    probe = InputContractPolicy(config['depth'], config['width'])
    require(sum(p.numel() for p in probe.parameters()) == report['allocated_parameters'], 'allocated parameters')
    for checkpoint in report['checkpoints']:
        require(sha(checkpoint['path']) == checkpoint['sha256'], f'checkpoint: {checkpoint["path"]}')
        saved = torch.load(checkpoint['path'], map_location='cpu', weights_only=False)
        require(saved['schema'] == SCHEMA and saved['protocol_sha256'] == report['protocol_sha256']
                and saved['job'] == report['job'] and saved['next_group'] == checkpoint['next_group'], 'checkpoint metadata')
        probe.load_state_dict(saved['policy'], strict=True)
        require(parameter_hash(probe) == checkpoint['policy_sha256'] == saved['policy_sha256'], 'checkpoint weights')
        optimizer = saved['optimizer']
        require(len(optimizer['param_groups']) == 1 and optimizer['param_groups'][0]['lr'] == config['learning_rate'], 'optimizer contract')
        state = optimizer['state']
        require((not state) if checkpoint['next_group'] == 0 else len(state) == len(list(probe.parameters())), 'optimizer state membership')
        for entry in state.values():
            require(float(entry['step']) == checkpoint['next_group'] and torch.isfinite(entry['exp_avg']).all()
                    and torch.isfinite(entry['exp_avg_sq']).all() and (entry['exp_avg_sq'] >= 0).all(), 'optimizer accumulated steps')
        if checkpoint['next_group']:
            group = report['groups'][checkpoint['next_group'] - 1]
            require(group['policy_after_sha256'] == checkpoint['policy_sha256'] and
                    group['policy_before_sha256'] == report['checkpoints'][checkpoint['next_group'] - 1]['policy_sha256'], 'checkpoint chain')
    for group in report['groups']:
        require(read(Path(group['checkpoint']['path']).with_name('group.json')) == group, 'group record')
        require(relative_returns(group['scores']) == group['returns'], 'relative returns')
        rows = [r for r in report['rows'] if r['phase'] == 'training' and r['group'] == group['group']]
        require([r['attempt'] for r in rows] == list(range(4)), 'complete episode group')
        require([r['best_metrics']['score'] for r in rows] == group['scores'], 'recorded terminal scores')
        require(all(r['policy_sha256'] == group['policy_before_sha256'] for r in rows), 'frozen group policy')
    expected = {(update, case, bank) for update in config['evaluate_at'] for case in protocol['evaluation_cases'] for bank in config['evaluation_banks']}
    actual = [(r['update'], r['case'], r['bank']) for r in report['rows'] if r['phase'] == 'evaluation']
    require(len(actual) == len(expected) and set(actual) == expected, 'evaluation membership')
    evidence_path = path.with_name('evidence.json')
    require(sha(evidence_path) == report['evidence_sha256'], 'evidence manifest')
    evidence = read(evidence_path)
    required = {str((Path(row['directory']) / name).resolve()) for row in report['rows'] for name in ('episode.json.gz', 'best.json.gz')}
    require(set(evidence) == required, 'evidence membership')
    for name, expected_hash in evidence.items():
        require(sha(name) == expected_hash, name)
    for row in report['rows']:
        episode = unpack(Path(row['directory']) / 'episode.json.gz')
        witness = unpack(Path(row['directory']) / 'best.json.gz')
        require(row['selected'] == episode['selected'] == config['horizon'] and row['budget_complete'] and episode['budget_complete'], 'episode budget')
        require(episode['best_metrics'] == row['best_metrics'] and episode['initial_metrics'] == row['initial_metrics'], 'episode scores')
        require(digest(witness) == episode['best_sha256'] == episode['trace'][-1]['best_sha256'], 'best witness identity')
        require(episode['policy_sha256'] == row['policy_sha256'], 'episode policy')
        require(Counter(t['action']['kind'] for t in episode['trace']) == row['kinds'], 'action counts')
        require(all(t.get('acceptance', {}).get('candidate_validated') for t in episode['trace'] if t['status'] in ('improved', 'accepted_equal', 'accepted_exploration')), 'native acceptance records')
        if not report['contract']['restore']:
            require(not row['kinds'].get('restore_best'), 'no-restore action absent')
    return report


def replay(runtime, protocol, report, row, guard):
    config = protocol['config']
    before_group = row['group'] if row['phase'] == 'training' else row['update']
    saved = next(c for c in report['checkpoints'] if c['next_group'] == before_group)
    checkpoint = torch.load(saved['path'], map_location='cpu', weights_only=False)
    require(checkpoint['schema'] == SCHEMA and checkpoint['job'] == report['job'] and checkpoint['next_group'] == before_group, 'replay checkpoint metadata')
    model = InputContractPolicy(config['depth'], config['width'])
    model.load_state_dict(checkpoint['policy'], strict=True)
    require(parameter_hash(model) == row['policy_sha256'], 'replay policy')
    case = next(c for c in protocol['cases'] if c['id'] == row['case'])
    episode = unpack(Path(row['directory']) / 'episode.json.gz')
    env = FactoryEpisode(runtime, case, config['horizon'], config['episode_seconds'], config['route_work'])
    memory, history = SelectionMemory(env.state['current']), []
    rng = torch.Generator().manual_seed(row['sample_seed'])
    for ordinal, old in enumerate(episode['trace']):
        guard()
        description = env.describe()
        keys, allowed, exhausted = selection_mask(env.state, description, memory, row['candidate'], config['memory'])
        obs = observation(env.state, description, 1 - ordinal / config['horizon'], history, row['candidate'], runtime)
        require(observation_hash(obs) == old['observation_sha256'], f'observation {row["directory"]}:{ordinal}')
        require(digest(allowed) == old['allowed_sha256'] and exhausted == old['memory_exhausted'], 'replay mask')
        with torch.set_grad_enabled(row['phase'] == 'training'):
            dist = distribution(model(obs) / config['temperature'], description['actions'], allowed, config['balanced'])
            index = int(torch.multinomial(dist.probs.detach(), 1, generator=rng))
            require(index == old['vocabulary_index'] and description['actions'][index] == old['action'], 'replay sampled action')
            require(probability_hash(dist.probs) == old['probabilities_sha256'] and float(dist.log_prob(torch.tensor(index)).detach()) == old['old_log_probability'], 'replay distribution')
        token = selected_action_token(obs, index)
        before, before_score = physical_key(env.state['current']), env.state['current_metrics']['score']
        require(digest(env.state['current']) == old['current_before_sha256'], 'replay current before')
        step = env.step(old['action'])
        require(step['status'] == old['status'] and step['current_score'] == old['current_score']
                and step['best_score'] == old['best_score'] and digest(env.state['current']) == old['current_sha256']
                and digest(env.state['best']) == old['best_sha256'], 'native replay')
        memory.record(before, physical_key(env.state['current']), keys[index])
        history.append(history_record(token, before_score, step, env.state['initial_metrics']['score']))
        history = history[-16:]
    require(env.state['best_metrics'] == row['best_metrics'] and parameter_hash(model) == row['policy_sha256'], 'replay endpoint')
    return dict(job=report['job']['id'], directory=row['directory'], case=row['case'], phase=row['phase'],
                selected=len(env.trace), observation_and_sampling_exact=True, native_replay_passed=True)


def comparison(rows, update, split=None):
    rows = [r for r in rows if r['phase'] == 'evaluation' and (split is None or r['split'] == split)]
    initial = {(r['initialization_seed'], r['case'], r['bank']): r for r in rows if r['update'] == 0}
    terminal = [r for r in rows if r['update'] == update]
    if not terminal:
        return None
    counts, initial_area, terminal_area, paired = Counter(), 0.0, 0.0, []
    for row in terminal:
        before = initial[row['initialization_seed'], row['case'], row['bank']]
        a, b = before['best_metrics']['score'], row['best_metrics']['score']
        counts['wins' if b < a else 'losses' if b > a else 'ties'] += 1
        initial_area += 100 * (1 - a[0] / row['initial_metrics']['score'][0])
        terminal_area += 100 * (1 - b[0] / row['initial_metrics']['score'][0])
        paired.append(dict(seed=row['initialization_seed'], case=row['case'], bank=row['bank'], initial_score=a, score=b))
    n = len(terminal)
    return dict(update=update, samples=n, wins=counts['wins'], ties=counts['ties'], losses=counts['losses'],
        initial_area_reduction_percent=initial_area / n, area_reduction_percent=terminal_area / n,
        paired_area_delta_pp=(terminal_area - initial_area) / n,
        pose_selections=sum(r['kinds'].get('pose', 0) for r in terminal),
        pose_improvements=sum(r['improvements'].get('pose', 0) for r in terminal),
        executor_rejections=sum(r['statuses'].get('executor_rejected', 0) for r in terminal), paired=paired)


def summarize(args):
    began = time.monotonic()
    if not 0 < args.seconds <= 7200:
        raise ValueError('input-summary.time-bound')
    if args.output.exists():
        raise ValueError(f'input-summary.existing-output: {args.output}')
    protocol, run = read(args.screen / 'protocol.json'), read(args.screen / 'run.json')
    config_check(protocol['config'])
    verify_sources(protocol)
    require(run['status'] == 'complete' and run['protocol_sha256'] == sha(args.screen / 'protocol.json'), 'completed run')
    require(len(run['rows']) == len(protocol['jobs']) and {r['job']['id'] for r in run['rows']} == {j['id'] for j in protocol['jobs']}, 'all candidate seeds')
    torch.set_num_threads(1)
    torch.use_deterministic_algorithms(True)
    runtime = None
    jobs, audited = [], []
    def guard():
        if time.monotonic() - began >= args.seconds:
            raise TimeoutError('input-summary.audit-bound')
    try:
        runtime = FactoryRuntime(protocol['native_library'], protocol['catalogs'])
        for run_row in run['rows']:
            guard()
            report = verify_job(args.screen, protocol, run_row)
            jobs.append(report)
            selections = [next(r for r in report['rows'] if r['phase'] == 'training' and r['group'] == 0 and r['attempt'] == 0),
                          next(r for r in report['rows'] if r['phase'] == 'training' and r['group'] == protocol['config']['groups'] - 1 and r['attempt'] == 3),
                          next(r for r in report['rows'] if r['phase'] == 'evaluation' and r['update'] == protocol['config']['groups'])]
            for row in selections:
                audited.append(replay(runtime, protocol, report, row, guard))
            print(dict(stage='audit', job=report['job']['id'], replayed=sum(r['selected'] for r in audited)), flush=True)
        candidates = []
        for name in protocol['config']['candidates']:
            selected = [r for j in jobs if j['job']['candidate'] == name for r in j['rows']]
            candidate_jobs = [j for j in jobs if j['job']['candidate'] == name]
            curves = [comparison(selected, update) for update in protocol['config']['evaluate_at']]
            endpoint = protocol['config']['groups']
            candidates.append(dict(candidate=name, contract=protocol['contracts'][name],
                curve=curves, training=comparison(selected, endpoint, 'training'),
                held_out=comparison(selected, endpoint, 'held-out-control'),
                per_seed=[dict(seed=seed, **comparison([r for r in selected if r['initialization_seed'] == seed], endpoint))
                          for seed in protocol['config']['seeds']],
                nonflat_groups=sum(not g['flat_returns'] for j in candidate_jobs for g in j['groups']),
                total_groups=sum(len(j['groups']) for j in candidate_jobs),
                mean_job_seconds=sum(j['seconds'] for j in candidate_jobs) / len(candidate_jobs),
                allocated_parameters=candidate_jobs[0]['allocated_parameters']))
        verify_sources(protocol)
        report = dict(schema_version=1, status='complete', smoke=protocol['smoke'],
            protocol_sha256=sha(args.screen / 'protocol.json'), run_sha256=sha(args.screen / 'run.json'),
            analysis_source_sha256=sha(__file__), audit_seconds=time.monotonic() - began,
            candidates=candidates, audited_episodes=audited, replayed_actions=sum(r['selected'] for r in audited),
            all_recorded_artifact_hashes_verified=True, total_jobs=len(jobs),
            scope='Paired exploratory screen. Native/observation/probability replay samples three episodes per job; other traces are checked by hashes and recorded acceptance. Two seeds and this short exposure do not establish convergence or generalization.')
        write(args.output, report)
        print(dict(status='complete', candidates=len(candidates), replayed=report['replayed_actions']), flush=True)
    except BaseException as error:
        write(args.output.with_suffix('.partial.json'), dict(status='incomplete', code='input-summary.failed', severity='error',
            stage='summary-audit', entity=str(args.screen), message=str(error), audited_episodes=audited))
        raise
    finally:
        if runtime is not None:
            runtime.close()


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--screen', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--seconds', type=int, default=3600)
    summarize(parser.parse_args())
