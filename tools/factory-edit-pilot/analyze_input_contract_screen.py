"""Describe reward coverage and reset-specific outcomes of an audited input screen."""
import argparse
from collections import Counter
import json
from pathlib import Path

from full_factory_input_summary import comparison, require
from full_factory_screen import sha


def read(path):
    return json.loads(Path(path).read_text())


def prepared(row):
    return row['case'].endswith('--contraction-exhausted')


def reward_coverage(jobs, is_prepared):
    counts = Counter()
    for job in jobs:
        for group in job['groups']:
            rows = [r for r in job['rows'] if r['phase'] == 'training'
                    and r['group'] == group['group']]
            require(len(rows) == 4 and len({r['case'] for r in rows}) == 1,
                    'supplement.group-membership')
            if prepared(rows[0]) != is_prepared:
                continue
            counts['groups'] += 1
            counts['nonflat_score_groups'] += not group['flat_returns']
            counts['different_area_groups'] += len({s[0] for s in group['scores']}) > 1
            for row, reward in zip(rows, group['returns']):
                counts['episodes'] += 1
                counts['area_improving_episodes'] += (
                    row['best_metrics']['score'][0] < row['initial_metrics']['score'][0])
                counts['positive_return_episodes'] += reward > 0
                if reward > 0:
                    counts['positive_with_improving_pose'] += row['improvements'].get('pose', 0) > 0
                    counts['positive_only_contraction_improvements'] += (
                        set(row['improvements']) == {'contract'})
    keys = ('groups', 'nonflat_score_groups', 'different_area_groups', 'episodes',
            'area_improving_episodes', 'positive_return_episodes',
            'positive_with_improving_pose', 'positive_only_contraction_improvements')
    return {key: counts[key] for key in keys}


def run(args):
    require(not args.output.exists(), f'supplement.existing-output: {args.output}')
    summary = read(args.summary)
    run_report = read(args.screen / 'run.json')
    protocol = read(args.screen / 'protocol.json')
    require(summary['status'] == 'complete' and not summary['smoke']
            and summary['all_recorded_artifact_hashes_verified'], 'supplement.audited-summary')
    require(summary['run_sha256'] == sha(args.screen / 'run.json')
            and summary['protocol_sha256'] == sha(args.screen / 'protocol.json'), 'supplement.identity')
    jobs = []
    for row in run_report['rows']:
        require(sha(row['report']) == row['report_sha256'], 'supplement.job-identity')
        jobs.append(read(row['report']))
    candidates = []
    for name in protocol['config']['candidates']:
        selected_jobs = [j for j in jobs if j['job']['candidate'] == name]
        rows = [r for job in selected_jobs for r in job['rows']]
        evaluations = []
        for update in protocol['config']['evaluate_at']:
            selected = [r for r in rows if r['phase'] == 'evaluation' and r['update'] == update]
            kinds, improvements, statuses = Counter(), Counter(), Counter()
            for row in selected:
                kinds.update(row['kinds'])
                improvements.update(row['improvements'])
                statuses.update(row['statuses'])
            evaluations.append(dict(update=update, episodes=len(selected),
                selected=sum(r['selected'] for r in selected),
                kinds=dict(kinds), improvements=dict(improvements), statuses=dict(statuses)))
        candidates.append(dict(candidate=name,
            reward_coverage={label: reward_coverage(selected_jobs, flag)
                             for label, flag in [('original', False), ('prepared', True)]},
            reset_curves={label: [comparison([r for r in rows if prepared(r) == flag], update)
                                 for update in protocol['config']['evaluate_at']]
                          for label, flag in [('original', False), ('prepared', True)]},
            evaluations=evaluations))
    output = dict(schema_version=1, status='complete', summary_sha256=sha(args.summary),
        analysis_source_sha256=sha(__file__), candidates=candidates,
        scope='Descriptive supplement to the audited exploratory screen. Positive episode ranks '
              'and recorded improving action kinds do not establish causal credit or learning gains.')
    args.output.write_text(json.dumps(output, indent=2, allow_nan=False) + '\n')
    print(dict(status='complete', candidates=len(candidates), output=str(args.output)), flush=True)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--screen', type=Path, required=True)
    parser.add_argument('--summary', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    run(parser.parse_args())
