"""Predictive value errors against realized returns, before any episode fitting."""
import math


def moments(pairs):
    errors = [prediction - actual for prediction, actual in pairs]
    return dict(count=len(pairs), prediction_sum=math.fsum(p for p, _ in pairs),
                return_sum=math.fsum(r for _, r in pairs), return_square_sum=math.fsum(r*r for _, r in pairs),
                error_sum=math.fsum(errors), error_square_sum=math.fsum(e*e for e in errors),
                absolute_error_sum=math.fsum(abs(e) for e in errors))


def episode(trace):
    remaining = 0.
    pairs = []
    for step in reversed(trace):
        remaining += step['reward']
        prediction = step['old_value']
        if not math.isfinite(prediction + remaining):
            raise ValueError('ppo-combinations.nonfinite-value: predictive diagnostic requires finite values and rewards')
        pairs.append((prediction, remaining))
    pairs.reverse()
    return {name: moments(selected) for name, selected in
            (('all', pairs), ('initial', pairs[:1]), ('early', pairs[:8]), ('late', pairs[8:]))}


def summarize(value):
    n = value['count']
    if not n:
        return dict(count=0, mean_prediction=None, mean_return=None, bias=None, rmse=None, mae=None, explained_variance=None)
    bias = value['error_sum'] / n
    variance = max(0., value['return_square_sum']/n - (value['return_sum']/n)**2)
    residual_variance = max(0., value['error_square_sum']/n - bias*bias)
    return dict(count=n, mean_prediction=value['prediction_sum']/n, mean_return=value['return_sum']/n,
                bias=bias, rmse=math.sqrt(value['error_square_sum']/n), mae=value['absolute_error_sum']/n,
                explained_variance=1-residual_variance/variance if variance > 1e-12 else None)


def aggregate(rows):
    selected = [r['critic_prediction'] for r in rows if 'critic_prediction' in r]
    if not selected:
        return {}
    result = {}
    for name in ('all', 'initial', 'early', 'late'):
        combined = {key: math.fsum(r[name][key] for r in selected) for key in selected[0][name]}
        combined['count'] = int(combined['count'])
        result[name] = summarize(combined)
    return dict(critic_prediction=result)
