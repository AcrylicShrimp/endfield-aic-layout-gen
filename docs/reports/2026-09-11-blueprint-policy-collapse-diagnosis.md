# Early movement suppression is not an explicit failure penalty

The user's explicit-penalty hypothesis is not supported by this run. The first
160 batches contain 40,960 recorded actions, including 2,063 rejected actions,
and **no negative rewards**. Rejected actions receive zero. The stronger local
explanation is a PPO policy update driven mostly by critic-derived credit in
zero-reward batches, with normalization increasing that signal relative to the
other actor losses. This explains measured early movement suppression; it is not
a complete explanation of long-term training quality.

The active twelve-hour trainer was neither stopped nor modified. All analysis
uses immutable recorded inputs and private learner copies. The
[diagnostic contract](../designs/2026-09-11-blueprint-policy-collapse-diagnosis.md)
precedes the counterfactual runs; the fixed attribution cutoff is update 160.

## Direct reward and implicit policy credit are different

`full_factory/blueprint/session.rs` initializes each action reward to zero and
assigns a nonnegative area improvement only on a validated improving commit.
`blueprint_ppo_collect.py` records that native reward directly. Although the
repository has a `failure_penalty` implementation in `ppo_reward.py`, this
blueprint runner does not call it. Clearing a nonexistent failure penalty would
leave these recorded rewards unchanged.

PPO nevertheless computes signed advantages from rewards and critic predictions:

```text
TD residual = reward + gamma * next_value - value
GAE = discounted accumulation of those residuals
actor advantage = (GAE - batch_mean) / max(batch_standard_deviation, 1e-8)
```

Thus zero reward does not imply zero actor credit. Of the 160 batches, 146
(91.25%) contain no positive reward. The first seven contain only zero rewards;
success replay does not begin until update nine. Initial movement suppression
therefore precedes any imitation gradient.

Aggregate selected-action credit is also inconsistent with uniform failure
punishment. These are descriptive sample means, not estimates of action quality:

| Recorded outcome | Actions | Mean normalized advantage | Negative rewards |
|---|---:|---:|---:|
| Pose accepted into private draft | 894 | -0.1552 | 0 |
| Pose rejected | 21 | +0.4431 | 0 |
| Single-route repair rejected | 129 | +0.1713 | 0 |
| Repair-all rejected | 669 | +0.4393 | 0 |
| Commit rejected | 1,244 | +0.4521 | 0 |

A draft-accepted pose can still leave overlap or pending connections. It is not
necessarily a useful move or a physically validated factory. Conversely, the
positive mean advantage of failures does not make those failures desirable.
It demonstrates why rejection status cannot be equated with a negative reward
or a consistently negative PPO label in this experiment.

## Movement probabilities fall on identical states

The probes reuse two initial training observations and the first recorded
observation of every iteration-zero evaluation case. Geometry, candidate lists,
time features and identities remain exactly fixed while only the checkpoint
changes. The probe digest is checked before and after all measurements.

| Checkpoint | Pose probability, two training starts | Pose probability, 32 evaluation starts |
|---|---:|---:|
| Untrained | 21.97% | 21.88% |
| 4 | 13.88% | 14.37% |
| 8 | 6.24% | 6.97% |
| 12 | 2.03% | 2.37% |
| 96 | 0.20% | 0.65% |
| 128 | 3.36% | 4.14% |
| 160 | 0.49% | 0.67% |

All these fixed probes still offer pose candidates: 55–60 on the two training
starts and 17–857 on the evaluation starts. Candidate disappearance and different
visited states cannot explain this fixed-state decline. Initial-state
probability is not the same statistic as action frequency over complete episodes.

Actual mean evaluation area reduction falls from 0.8166% to zero at update 12,
remains zero through 120, briefly reaches 0.7386% at 124, and falls again. A
separate later snapshot through evaluation 320 records intermittent recovery,
including values above the untrained score, rather than permanent collapse.
It ends at 0.2841%. This later snapshot is observational and is outside the
160-update attribution cutoff; it is not evidence that training has converged.

## Exact local interventions identify the influential loss

Seven selected updates (1, 2, 4, 9, 41, 97 and 129) were replayed from their
original behavior model, both Adam states and shuffle RNG. Reference replays
match the original final model, both optimizer digests and shuffle digest.
Each counterfactual owns deep-copied optimizer state and uses the same recorded
samples, targets and initial optimizer state. It collects no new trajectories.

The following values are changes in pose probability on the same 32 initial
evaluation states, in percentage points, after that one optimizer update:

| Intervention | Update 1 | Update 9 | Update 129 |
|---|---:|---:|---:|
| Actual update | -3.0093 | -1.8962 | -1.1690 |
| Omit self-imitation | -3.0093 | -1.9632 | -1.3240 |
| Detach critic input from shared encoder | -3.0041 | -1.8961 | -1.1723 |
| Zero PPO surrogate advantage; keep other losses | +7.8156 | +0.8627 | -0.3614 |
| Omit entropy regularization | -3.0852 | -1.9240 | -1.3176 |
| Center GAE without dividing by standard deviation | +6.2399 | +0.8543 | -0.3686 |

These selected updates all have zero collected reward. The standard-deviation
multipliers are approximately 140, 486 and 727 respectively. Those are multipliers
on centered advantage, **not effective learning-rate multipliers**: Adam, clipping,
other loss components and retained momentum intervene.

This is direct local evidence that the PPO surrogate and its scaling can suppress
movement in these zero-reward updates. Removing only the critic loss's direct
gradient into the shared encoder barely changes the result. That does not remove
the critic's influence through GAE targets and advantages.

Self-imitation partially offsets suppression in the selected nonempty-buffer
updates; it does not cause the early decline. At update nine, replay contains one
self-discovered sequence with one pose, one repair and one commit label. The
first actual minibatch's coefficient-weighted imitation actor-gradient norm is
0.00321 versus 0.17436 for the PPO component. Those are unclipped first-minibatch
Euclidean norms, not total-update causal shares.

PPO does not always suppress pose: update 41 increases it, and deleting its
surrogate largely removes that increase. The first-minibatch gradient alignment
can also differ from the net four-epoch Adam update. The complete counterfactuals,
not a single gradient sign, support the local direction claims above.

## Remaining limits and the next bounded comparison

The evidence supports investigating critic-derived credit and its scale during
initial sparse-reward learning. It does not prove that removing normalization
improves validated area, nor that this would be stable across seeds. A bounded
follow-up should compare the current normalization against centered, unscaled
GAE from fresh matched initializations, keeping the 64-action episodes, training
inputs, evaluation cohort and action budget fixed. Measure area, positive commits,
pose probability and critic calibration; do not select a winner on pose frequency
alone. This follow-up has not been applied to the live run.

There is also a training-distribution limitation: training repeatedly uses the
same five-facility crossing fixture in original and overlap starts, while the
restored evaluation comprises 32 distinct factories. This is an observed input
mismatch, not a causal explanation established by this ablation. The fixed-state
decline on the training starts shows that evaluation generalization alone cannot
explain the initial suppression.

## Verification, reproducibility and retention

- Seven exact reference update replays and 35 loss-term counterfactuals completed.
  Empty-buffer removal of imitation is exactly identical to the reference.
- A diagnostic validation issue was caught before interpreting the interventions:
  PyTorch optimizer restoration can share supplied Adam tensors. The diagnostic
  now deep-copies every arm, asserts checkpoint immutability and checks empty-buffer
  parity. The discarded draft ablation output is not retained as evidence.
- Two new regression tests check Adam isolation and nonzero normalized GAE from
  zero rewards with unequal value predictions. The full Python suite passes:
  409 tests run, seven skipped.
- Rust workspace check and tests pass (564 passed, five ignored). No runtime,
  native semantics, dependencies, constructor or global exact research changed.
- Input/chunk/probe identities, finite gradients and source bindings are checked.
  This analysis replays seven optimizer updates; it is not a new full native audit
  of all 40,960 actions. Native validation remains the running evaluator's contract.

```sh
tools/factory-edit-pilot/.venv/bin/python tools/factory-edit-pilot/diagnose_blueprint_policy.py \
  --run artifacts/factory-edit-pilot/blueprint-success-64-12h-corrected-20260911 \
  --through 160 --output NEW_DIAGNOSTIC_DIRECTORY
tools/factory-edit-pilot/.venv/bin/python tools/factory-edit-pilot/summarize_blueprint_policy.py \
  --diagnosis NEW_DIAGNOSTIC_DIRECTORY
```

Artifacts are retained at
`artifacts/factory-edit-pilot/blueprint-policy-diagnosis-20260911/`:
`summary.json`, `inputs.json`, `compact.json`, `later-evaluations.json` and
`diagnosis.png`. The full input manifest binds every consumed immutable record
and checkpoint; the [compact provenance](2026-09-11-blueprint-policy-diagnosis-provenance.json)
binds the analysis outputs. Regeneration without `--live-run` reuses the retained
later-evaluation snapshot when present. Adding `--live-run RUN` intentionally
captures a new observation time, not a replay of that later snapshot.
