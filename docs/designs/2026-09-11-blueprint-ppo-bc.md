# Blueprint PPO with a small behavior-cloning auxiliary loss

## Status and question

Registered before implementation and execution on 2026-09-11. The user approved
mixing supervised learning after the post-pose diagnosis found discarded,
recoverable area improvements. Does a small demonstration loss help a policy
complete useful moves during autonomous inference?

## Data contract

Inputs are the audited, frozen persistent PPO training run and post-pose recovery
census. No evaluation trace supplies labels. Examine every recorded area-improving
post-pose case. Exclude repair-ready and already-improved starts. Reconstruct the
authenticated pre-pose native checkpoint from its original chunk. Independently
run original-budget `repair_all, commit` before the pose, and
`pose, repair_all, commit` after it. Retain a demonstration only when both branches
fully validate and the latter has strictly smaller area than the former and the
retained best. A failed before branch is inconclusive, not proof of useful motion.
Preserve original topology, poses outside the selected move, routing bounds,
counters and charged clock. Each label must be an actual policy candidate.

Output is a versioned dataset with full pre-action observations, candidate inputs,
action indices, native outcomes, before/after scores, source references, native
checkpoints and validated witnesses. Validate witnesses independently through
complete factory admission. Deduplicate by physical pre-pose draft, retained best
and move, ignoring clock/counter features only for grouping. Group by physical
pre-pose draft and retained best, then deterministically assign every fifth sorted
hash group to holdout. Require nonempty training and holdout groups. All three
action stages are sampled together, with uniform demonstration sampling, so
recovery labels do not overwhelm movement labels. Holdout is within this fixture,
not evidence of generalization. Dataset scarcity is a reported result.

## Learner and run contract

Use the existing blueprint graph model, native contracts, reward, candidate set,
four starts and persistent episodes unchanged. Two fresh model seeds (17, 29),
64 updates, eight streams, 32 actions per chunk and 256 actions per episode give
16,384 on-policy actions per seed per arm. Arms differ only in BC coefficient:
control 0 and treatment .05. Both start from identical model/optimizer and sampler
states; wall-clock features prevent a claim of identical realized trajectories.
The existing PPO settings remain unchanged, including learning rate 3e-4.

For each actor-active minibatch, sample eight training demonstrations uniformly
with replacement using an independent deterministic seed/update generator and
add coefficient times mean negative log probability over their complete three-step
sequences. BC uses the same kind-balanced policy distribution as inference.
Teacher rows never enter PPO ratios, GAE, rewards or critic targets. BC may update
the shared encoder and actor, but has no direct critic-head gradient. Once PPO
KL stopping freezes the actor/encoder, BC also stops; the critic-only tail remains
unchanged. Record BC loss, selected demonstration IDs, update/model/Adam/RNG hashes,
KL and actor epochs. Checkpoints plus frozen inputs support exact update replay.

Evaluate both arms at updates 0/16/32/64 using the existing two-second native
session budget, original/overlap/improved starts and two fixed sampling streams
per stratum. No teacher or automatic completion runs during policy evaluation.
Also measure held-out action likelihood by stage and bounded autonomous completion
from held-out pre-pose and post-pose checkpoints (64 actions, 2000 ms). Use
checkpoint 0 and 64 and two independent fixed sampling streams per held-out case.
Report action-budget quality as well as elapsed-time quality and movement share.
Primary outcome: final mean validated area reduction on ordinary autonomous
inference, with per-seed and per-start results. Secondary: held-out completion,
likelihood, useful-move continuation and action mix. Two seeds on one fixture are
a bounded screen, not a significance or generalization claim. Do not retune after
seeing results. Allow 1800 seconds and 2 GiB per arm; retain non-replayable evidence.

## Verification and failures

Test coefficient-zero equivalence to the original update, BC actor/encoder gradient
with no direct critic gradient, actor freeze after KL stopping, and exact replay
from nonempty Adam. Run a short native readiness job and replay its updates before
the screen. Replay every screen update and independently validate all final bests
and successful commits. Compare initial model hashes across arms. Preserve source
hashes and historical runtime files; use an explicit experimental BC runner.

Stable failures use `blueprint.bc-data`, `blueprint.bc-update`, `blueprint.bc-run`
or `blueprint.bc-audit` prefixes, with input path/entity where relevant. Reject
changed source hashes, absent candidates, budget exhaustion, non-finite losses,
invalid witnesses, split leakage and replay mismatch. Retain failed-run state.
Outputs belong under ignored `artifacts/`; commit scripts, external configuration,
this contract, a concise report and compact provenance only. Production construction
and optimization are unchanged and retain the connected transactional contract in
`docs/designs/2026-09-02.00-constructive-layout-planner.md`.
