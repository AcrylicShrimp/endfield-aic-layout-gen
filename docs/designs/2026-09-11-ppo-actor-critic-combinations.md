# PPO actor/critic combinations

Status: experiment contract, written before implementation and learning.

The user stopped the padded run after 258 batches and requested combinations of
encoder sharing, critic action history, and critic learning after policy KL
stopping. This is a bounded learning screen, not a replacement of the production
constructive planner or an assumption that sharing is superior.

## Factors and controls

Run the full 2 x 2 x 2 matrix, with initialization seeds 17 and 29:

| Factor | Control | Treatment |
| --- | --- | --- |
| Backbone | Independent parameters | Shared CNN, graph encoder and history encoder |
| Critic history | Zero history features | Same last 16 action tokens available to actor |
| Critic schedule | Stop with actor | Finish four critic epochs after actor stops |

The actor always uses `current-history-16`. The critic always sees full current
and best worlds, remaining horizon, score context and allowed-action summaries.
Only the history factor controls its pre-action history features. No action
outcomes, future state or future reward enter either prediction. Best geometry
remains critic-only even when weights are shared; a shared parameter does not
grant the actor another observation.

The shared model reuses current-world features and, when applicable, recurrent
history features between predictions. Its critic may additionally encode the
best world. Actor-specific member/scoring layers and the scalar value head
remain private. Separate models have independent copies of the common encoders.
Initialize those copies identically to the actor encoders to isolate sharing
from initial representation differences. Allocate the same history-capable critic
head in every cell, masking history to zero in controls. Per-seed initial actor
weights and predictions, and initial zero critic outputs, must match all cells.
This factorial control is not an exact continuation of the previous critic.

## Optimization contract

Keep gamma 1, GAE lambda 0.95, minibatch/microbatch 64, Adam learning rate 0.0003,
PPO clip 0.2, target KL 0.02, entropy coefficient 0.001 and gradient norm cap 0.5.
Keep the current direct best-area reward and failure penalty 0.001. Freeze
behavior values, GAE, standardized advantages and value targets for each update.

Each unique parameter belongs to exactly one optimizer. During joint training,
shared encoders receive the sum of policy and value gradients and are stepped
once. The actor optimizer owns shared parameters; the critic optimizer owns only
its private parameters. Gradient clipping must not clip a shared tensor twice.
The two disjoint optimizer groups retain the existing separate norm limits.
The existing half-MSE value loss has coefficient one in the combined objective.

Check policy KL after each actor epoch. Both schedule cells perform identical
updates through that check. In the continued-critic cell, suppress subsequent
actor updates and finish the remaining critic epochs, without recomputing GAE.
Freeze every actor parameter during that tail. With independent networks the
whole critic can learn; with shared encoders only the private critic head can
learn. Report this interaction explicitly rather than claiming identical critic
capacity in both tails. Actor parameters and optimizer state must remain exact
through the tail. This also prevents critic fitting from bypassing policy KL.

Use a deterministic tail shuffle stream derived from the joint shuffle state
without advancing the joint stream. Thus extra critic epochs do not change the
next batch's actor minibatch order solely through random-number consumption.

## Inputs, budget and measurements

Each of 16 runs receives 32 batches of 64 factory attempts and 32 actions per
accepted episode: at most 65,536 training actions per run, 1,048,576 total.
Use fresh initializations, matched generation/action seeds within initialization
seed, and fresh factory starts without policy-dependent start inheritance in this
screen. Verify admission and initial-geometry identities across cells. This
isolates the three factors; it does not establish their effects under the earlier
80/10/10 inherited-state curriculum.

Evaluate at batches 0, 8, 16, 24 and 32 on 32 held-out generation seeds per
initialization seed, with identical action RNG seeds across cells/checkpoints.
Evaluate all cells without selecting a winner early. These reused cohorts support
paired screening comparisons, not an untouched final confirmation test.

Record best-area reduction, first-eight and later-24 contributions, action kinds,
rejections, KL, actor/critic epoch and optimizer-step counts, phase/update time,
and resource limits. Record pre-update behavior value predictions against actual
remaining Monte Carlo rewards, including failures, for all, initial, early and
late positions. Also record these predictions on evaluation episodes, which never
train. Keep training-target fit distinct from those predictive errors. A sampled
return includes policy randomness; residuals alone do not prove miscalibration.

Use sequential GPU cells, sixteen CPU workers, CUDA FP32, existing per-task and
per-batch watchdogs, 32 GiB process-tree RSS and 16 GiB free-disk reserve. Bound
each two-seed cell to two hours. Preserve every checkpoint; retain training
traces/experiences at batches 1, 16 and 32 and every evaluation trace. Disposable
outputs belong in ignored artifacts; retain compact human findings and hashes.

## Verification and acceptance

- Validate the versioned three-factor model contract and parameter ownership.
- Compare scalar and padded predictions and gradients, including real variable
  world shapes, nonempty history, masks, best-world isolation and shared storage.
- Verify critic-only tails preserve actor parameters, Adam and joint shuffle;
  check exact checkpoint continuation with nonempty optimizer state.
- Verify native observation/history correspondence and a bounded CUDA readiness
  run covering an actual KL stop and critic-only tail before matrix execution.
- Run relevant Python tests, workspace checks/tests and artifact-policy checks.
- Freeze source, configuration, binaries and input seeds before the matrix;
  audit committed evidence, report every cell and both initialization seeds.
- Compare paired effects by seed and factor interactions. Do not infer a general
  winner from two seeds or substitute training value loss for factory improvement.

Failures use the existing structured harness diagnostic schema (code, severity,
stage, entity, message). Model and tail invariant failures use `ppo-combinations.*`;
batched-observation checks retain `ppo-training.contract`.
Partial or failed cells remain visible and are never reported as completed runs.
