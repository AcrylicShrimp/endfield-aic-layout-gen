# What peer-rank credit proves, and what it does not

Status: mathematical interpretation and decision constraint, 2026-09-07.
No change to the running cumulative experiment, its objective, or release requirements.

## Limited estimator result

Fix a complete factory and a behavior policy q. Let S(tau) be a trajectory's retained
`[area, physical tiles, turns, futile proposals]` tuple. Define u(a,b) as +1 when
S(a) is lexicographically smaller, -1 when larger, and zero on an exact tie.
For a candidate policy p_theta, define

`J(theta; q) = E[a ~ p_theta, b ~ q][u(a,b)]`.

During one on-policy update, draw four independent equal-length trajectories from q.
The mean of the three peer comparisons for each trajectory estimates its fixed-opponent
reward. The likelihood-ratio identity gives, at p_theta = q,

`E[(1/4) sum_i rank_i * grad log p_theta(tau_i)] = grad_theta J(theta; q)`.

The implemented pre-action prefix rank is an action-independent baseline: conditional on
the histories before the current actions, its product with the current action's log-policy
gradient has zero expectation. This requires independent trajectory sampling and no
post-action information leakage. Averaging over T actions adds a factor 1/T to the task
gradient; it does not change its direction. These statements concern the task estimator,
not the entropy term, clipping, Adam, or a finite sampled update's effectiveness.

The opponent distribution must remain fixed when differentiating. Differentiating both
arguments of J(theta; p_theta) instead gives zero by antisymmetry. Likewise, zero mean
peer rank within a group does not imply that the fixed-opponent policy gradient is zero.

With a nonzero exact gradient, differentiability and a sufficiently small plain ascent
step, the fixed-opponent J improves locally. This is not a convergence proof for the
moving-opponent neural learner, nor a guarantee about unseen factories or 1% empty space.
The general likelihood-ratio/policy-gradient basis is described by
[Sutton et al. (1999)](https://proceedings.neurips.cc/paper_files/paper/1999/file/464d828b85b0bed98e80ade0a5c43b0f-Paper.pdf).

## Counterexample to equating win rate with expected area

This is an abstract outcome distribution, not a measured factory or game-data claim.
Keep all lower-priority score components equal. Strategy A always returns area 100.
Strategy B returns area 99 with probability 3/5 and area 10000 with probability 2/5.
Thus u(B,A) has expectation 1/5, but expected area is 100 for A and 20297/5
(4059.4) for B. Pairwise preference discards the magnitude of both wins and losses.

Consider a one-parameter policy family choosing B with probability x, with the
conditional outcome distributions fixed. Against an opponent mixture fixed at x_old,
the derivative of J with respect to the candidate's x is 1/5 at every mixture.
The derivative of expected area is 19797/5 (3959.4): increasing x worsens it.
This discrepancy exists with an exact gradient, not just four-sample estimation noise.

Expected area is not being substituted for the project's configured objective here.
The example only disproves the implication that improving tournament preference must
improve expected area. It does not prove failure of every rank-based learner, failure
of this factory's action domain, or impossibility of the user's goal.

## Consequences for the next decision

- Bounded rank credit preserves each comparison's lexicographic priority and avoids
  cross-factory score-scale weighting. Those are design motivations, not efficacy proof.
- Four samples are a valid estimator construction, not a guarantee of adequate signal.
  If a useful outcome has per-trajectory probability p, its chance of appearing in an
  independent four-trajectory group is `1 - (1-p)^4`. Ranking cannot guarantee discovery.
- When all sampled areas tie, lower-priority differences can still drive updates.
  Nonzero gradients, prefix credit, or changing optimizer state do not prove packing
  learning. Prefix subtraction is not guaranteed to reduce variance in every setting.
- Preserve the preregistered 1024-update study. It measures finite empirical effectiveness
  of this configuration, not equivalence between its surrogate and the release objective.
- Before adopting any replacement credit, define its objective and its relationship to
  the unchanged area/tiles/turns priority, low waste, and the density/no-filler gates.
  State estimator assumptions and counterexamples, then use a controlled main-path
  comparison. Neither this critique nor a budget-curve plateau selects a replacement.
- Continue reporting score components, per-root regressions, futility, target density,
  and direct geometry inspection. Tournament wins alone cannot authorize release.

## Verification and limits

Re-ran `uv run --offline --frozen --project docs/benchmarks/layout-learning python -m unittest discover -s docs/benchmarks/layout-direct -p test_credit_reference.py -v`:
all five existing tests pass. They enumerate a two-step, four-path, four-worker model,
check the fixed-opponent gradient and prefix baseline, and include ties and a biased
post-action negative control. This is a finite reference check, not a factory convergence
theorem. Independently checked the counterexample with exact rational arithmetic.
No training, executor, evaluation, or pinned experiment-contract source was changed.
