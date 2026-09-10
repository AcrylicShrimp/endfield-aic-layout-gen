"""Explicit PPO area progress and weak failed-action cost accounting."""
import math

SCHEMA = 'ppo-best-area-with-failure-cost-v1'
FAILURES = frozenset(('executor_rejected', 'rejected', 'late'))
STATUSES = FAILURES | {'improved', 'accepted_equal', 'accepted_exploration'}
DEFAULT = {'failure_penalty': 0.0}


def require(condition, message):
    if not condition:
        raise ValueError(f'ppo-reward.contract: {message}')


def validate(config):
    require(isinstance(config, dict) and set(config) == {'failure_penalty'},
            'reward requires exactly failure_penalty')
    value = config['failure_penalty']
    require(type(value) in (int, float) and math.isfinite(value) and 0 <= value <= .01,
            'failure_penalty must be finite in [0, 0.01]')
    return config


def apply(step, config):
    validate(config)
    require(step['status'] in STATUSES, 'registered native action status required')
    area = step['reward']
    require(math.isfinite(area) and 0 <= area <= 1, 'native area reward must be in [0, 1]')
    cost = config['failure_penalty'] if step['status'] in FAILURES else 0.0
    require(step['status'] not in FAILURES or area == 0, 'failed action must preserve best area')
    step.update(area_reward=area, failure_cost=cost, reward=area - cost)


def totals(steps, config):
    validate(config)
    for step in steps:
        require(step['status'] in STATUSES, 'registered action status required')
        area, cost = step['area_reward'], step['failure_cost']
        require(math.isfinite(area) and 0 <= area <= 1, 'finite area reward')
        expected = config['failure_penalty'] if step['status'] in FAILURES else 0.0
        require(cost == expected and (step['status'] not in FAILURES or area == 0), 'failed-action cost')
        require(step['reward'] == area - cost, 'area progress minus failure cost')
    return dict(reward_schema=SCHEMA, reward_config=dict(config),
        reward_sum=math.fsum(s['reward'] for s in steps),
        area_reward_sum=math.fsum(s['area_reward'] for s in steps),
        failure_cost_sum=math.fsum(s['failure_cost'] for s in steps),
        failed_actions=sum(s['status'] in FAILURES for s in steps))


def validate_episode(steps, row, config):
    expected = totals(steps, config)
    for key, value in expected.items():
        actual = row[key]
        require(math.isclose(actual, value, abs_tol=1e-12) if type(value) is float else actual == value,
                f'episode accounting differs: {key}')
    initial, best = (row[key]['score'][0] for key in ('initial_metrics', 'best_metrics'))
    require(initial > 0 and 0 < best <= initial, 'positive monotone best area')
    require(math.isclose(expected['area_reward_sum'], (initial - best) / initial, abs_tol=1e-12),
            'area component must telescope independently of failure costs')
    return expected
