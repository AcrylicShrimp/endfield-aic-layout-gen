# Restore an explicit reinforcement-learning experiment

## Status

In progress. Report-30's on-policy return learner is implemented and its seven
focused gradient, native-rollout and failure-handling tests pass. Random training
completed 64 updates; imitation-warm training reached its 90-minute guard after
51 updates, with three unapplied episodes retained. All 118 checkpoints and 68
Python tests pass, as do workspace check and 538 Rust tests with two existing
ignores. Full physical replay passes all 463 episodes and 14816 actions. Prospective
admission retained six attempts and accepted two. Initial/terminal/geometry
evaluation completed 175 episodes under the report-32 termination amendment;
independent replay passes all 5600 actions and the final summary passes. Both arms
have 12 wins, 12 ties and 11 losses against initialization, with lower mean area
reduction. Sustained learned rearrangement remains unproven; design 33's bounded
parallel collector and subsequent experience-budget study are next. Completed
earlier training remains supervised imitation. Architecture and long-teacher
experiments remain paused.

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
- 2026-09-09: Define the return, distribution, gradient recomputation, resource and
  evaluation contracts in `docs/designs/2026-09-09.30-on-policy-factory-return-learning.md`
  before training. Start the frozen two-initialization study using complete original
  and prepared training factories. Seven focused tests verify lexicographic returns,
  delayed credit, zero reward gradients for ties, exact native rollout recomputation
  and bounded failure preservation. Independent full-trace/checkpoint audits,
  prospective evaluation and normal repository verification remain pending.
- 2026-09-09: The user confirms continuing RL and requests outcome monitoring over
  time. Add a read-only, bounded monitor with separate curves for each fixed
  original/prepared factory, observed episode ranges and balanced complete-cycle
  summaries. Three focused aggregation tests pass; inspect the live PNG/SVG. The
  first five complete cycles fluctuate without a clear aggregate upward trend.
  Keep this exploratory training evidence separate from frozen evaluation.
- 2026-09-09: The imitation-warm arm reaches its declared 5400-second guard after
  51 updates. Preserve the original partial report and all sampled episodes;
  do not extend training or claim 64 completed warm updates. Define the terminal
  analysis/evaluation amendment before prospective admission. Six new accounting
  tests verify completed/unapplied prefixes and failure rejection. All 118 unique
  checkpoints pass strict model/Adam/RNG inspection; start all-action physical
  replay, including the three unapplied episodes. The monitor stops successfully
  with five complete warm cycles; their mean area reductions are 9.908%, 10.126%,
  8.105%, 9.830% and 11.233%. Frozen behavioral comparisons remain pending.
- 2026-09-09: Complete independent physical replay of all 14816 sampled actions,
  including the three unapplied episodes. Record actual pose translations after
  accounting for canonical frame shifts. Start the guarded prospective admission,
  import and frozen evaluation sequence. The user highlights the small experience
  budget and suggests parallel collection; report 30 clarifies the inference limit
  and design 33 defines the next bounded concurrency comparison. Neither longer
  training nor the parallel collector has started.
- 2026-09-09: Complete the frozen 35-case, five-method evaluation with 175 episodes
  and 5600 selected actions. Preserve four failed prospective construction attempts.
  Start independent evaluation replay. Provisional initial/terminal comparisons
  are mixed; do not infer an RL ceiling from this small experience budget.
- 2026-09-09: Complete independent evaluation replay of all 175 episodes and 5600
  actions and finalize report 30 from the audited summary. Warm pose selections
  fall from 35 to 2, with no improving terminal pose or reroute; random terminal
  has four improving poses and two improving reroutes, versus six and three before
  training. Inspect the original Heavy and paired area extremes as whole factories;
  Heavy remains 30.582% empty and Medical extremes remain over 83% empty. Preserve
  snapshots required by the terminal-result loader. Monitoring is committed as
  `20f7672`. No fundamental RL impossibility or production readiness is established.
