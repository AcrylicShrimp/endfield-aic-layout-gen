"""Pure area progress, physical repeats and inherited-start group measurements."""
from collections import Counter

from ppo_reward import FAILURES


def episode_metrics(trace):
    by_kind = Counter()
    for step in trace:
        by_kind[step['action']['kind']] += step['area_reward']
    return dict(area_improving_actions=sum(t['area_reward'] > 0 for t in trace),
                accepted_physical_unchanged_actions=sum(t['status'] not in FAILURES and t['physical_unchanged'] for t in trace),
                physical_unchanged_actions=sum(t['physical_unchanged'] for t in trace),
                first_eight_area_reward_sum=sum(t['area_reward'] for t in trace[:8]),
                later_area_reward_sum=sum(t['area_reward'] for t in trace[8:]),
                area_reward_by_kind=dict(by_kind))


def progress_metrics(rows):
    if not rows or not all('area_improving_actions' in row for row in rows):
        return {}
    selected = sum(r['selected'] for r in rows)
    by_kind = Counter()
    for row in rows:
        by_kind.update(row['area_reward_by_kind'])
    return dict(area_improvement_fraction=sum(r['area_improving_actions'] for r in rows) / selected,
                accepted_physical_unchanged_fraction=sum(r['accepted_physical_unchanged_actions'] for r in rows) / selected,
                physical_unchanged_fraction=sum(r['physical_unchanged_actions'] for r in rows) / selected,
                mean_first_eight_area_reduction_percent=100 * sum(r['first_eight_area_reward_sum'] for r in rows) / len(rows),
                mean_later_area_reduction_percent=100 * sum(r['later_area_reward_sum'] for r in rows) / len(rows),
                mean_area_contribution_by_kind={k: 100 * v / len(rows) for k, v in by_kind.items()})


def curriculum_metrics(results):
    if not results or not all('start_curriculum' in r['generation'] for r in results):
        return {}
    groups = {}
    for depth in range(3):
        attempted = [r for r in results if r['generation']['start_curriculum']['depth'] == depth]
        complete = [r for r in attempted if r['status'] == 'complete']
        rows = [r['row'] for r in complete]
        selected = sum(r['selected'] for r in rows)
        group = dict(attempted=len(attempted), accepted=len(complete), selected=selected,
                     attempted_fraction=len(attempted) / len(results), **progress_metrics(rows))
        if complete:
            group.update(mean_area_reduction_percent=100 * sum(r['area_reward_sum'] for r in rows) / len(rows),
                         mean_failure_cost=sum(r['failure_cost_sum'] for r in rows) / len(rows),
                         failure_fraction=sum(r['failed_actions'] for r in rows) / selected,
                         mean_initial_area=sum(r['initial_metrics']['score'][0] for r in rows) / len(rows),
                         mean_prior_area_reduction_percent=100 * sum(1 - r['row']['initial_metrics']['score'][0] /
                             r['generation']['start_curriculum']['root']['initial_area'] for r in complete) / len(complete),
                         mean_cumulative_area_reduction_percent=100 * sum(1 - r['row']['best_metrics']['score'][0] /
                             r['generation']['start_curriculum']['root']['initial_area'] for r in complete) / len(complete))
        groups[str(depth)] = group
    return dict(start_depths=groups,
                start_requested_counts=dict(Counter(str(r['generation']['start_curriculum']['requested_depth']) for r in results)),
                start_fallback_counts=dict(Counter(r['generation']['start_curriculum']['fallback'] for r in results
                                                   if 'fallback' in r['generation']['start_curriculum'])))
