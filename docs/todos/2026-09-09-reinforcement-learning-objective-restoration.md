# Restore an explicit reinforcement-learning experiment

## Status

Pending implementation. Current completed training is supervised imitation. No new
RL run, architecture comparison or long-teacher collection has started after the
report-29 objective audit.

## Context

The user expects a policy that learns factory improvement through interaction and
reward. Recent teacher collection/imitation was incorrectly discussed as if it
tested that RL capability. Earlier bounded PG screens had weak signals; they do not
justify abandoning or declaring RL impossible. Preserve their evidence and the
supervised checkpoints, but name each learning method accurately.

## Completion criteria

- Define actual RL input/state/action contracts, terminal or incremental reward,
  return/advantage construction, sampling/masking distribution, horizon, resource
  accounting, optimizer update and diagnostic schema before implementation.
- Preserve complete original factory obligations, valid incumbents and configured
  lexicographic score priority; do not silently substitute an empty-fraction reward
  or a weighted score with different preferences.
- Verify the policy-gradient path without stored target actions on a small complete
  factory. Check reward-dependent gradients separately from entropy regularization,
  including an all-equal-return case and a delayed-return case.
- Retain sampled transitions and failures; account for observations, native routing,
  validation and model inference. Avoid retaining unbounded autograd graphs across
  large factory rollouts; any recomputation must preserve the sampled distribution.
- State whether initialization is random or a frozen imitation warm start. Compare
  trained and untrained policies under the same budgets and evaluation inputs.
- Measure sustained behavior and future transfer only after the basic RL signal is
  established. Keep report-28 prospective requests unopened until checkpoints freeze.
- Treat architecture/curriculum changes as separate hypotheses. Report effective
  reward updates, unchanged/invalid/revisited actions, final score and actual physical
  relocation/rerouting; do not call finite nonzero gradients task success.
- Preserve the original 59-facility Heavy Xiranite at-most-1% empty-XY and visual
  target plus the bounded CLI as the full unfinished objective.

## Activity log

- 2026-09-09: Report 28 finishes varied short-path collection; no model trained.
- 2026-09-09: User challenges the small visual change and asks whether learning is
  theoretically possible, then points out that RL does not require correct-action
  labels. Inspection confirms recent updates use supervised target-mass loss.
- 2026-09-09: Pause untrained architecture and longer-teacher drafts. Report 29
  records the objective distinction and a read-only exact-input conflict diagnostic:
  53 lessons, no detected identical-input or action-readout contradictions. This is
  limited negative evidence, not proof of full representability or RL learnability.
