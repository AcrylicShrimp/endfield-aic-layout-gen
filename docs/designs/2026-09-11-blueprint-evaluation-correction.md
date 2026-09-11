# Correct duration evaluation and restore shared tracking metrics

Status: implemented and verified; fresh twelve-hour launch follows. Preserve the stopped run's frozen inputs,
model, both optimizers, RNG, native state, experience and completed evaluations.

## Evaluation schedule

Replace the duration-v1 1,800-second evaluation trigger with an explicit
`evaluation.interval_batches = 4`. One batch is one optimizer update over eight
32-action stream chunks (256 actions). It is not a PPO minibatch or epoch. The
64-action episode spans two batches; evaluate at initialization, every four
completed batches, and final stop. Evaluation has its own sampler and cannot
alter training RNG, unfinished drafts, success labels or learner parameters.

The user confirmed restoration of the exact historical 32 factories. Preserve
all initial/request bytes and catalogs from protocol SHA-256
`fb6061b7513d3d558eb6beb21b0ab1a1346978d8c42d24023e820cb2b31b7531`.
Do not reuse the former original/overlap six-case evaluation. The training
episodes remain 64 actions; evaluation permits 64 actions and two native seconds
on each of the 32 original factories, with no artificial overlap prefix.

The user additionally requires fresh training: no trained weights, optimizer
state or success replay buffer is imported. Evaluate the actual untrained model
at iteration zero, commit and export that datapoint before the first learner
update, then start the new training stream. A random untrained policy may have a
nonzero area reduction; only its paired change from itself must be zero.

Register a duration-v2 external protocol. Reject v1 training/resume with an
explicit diagnostic referring to its frozen historical runner. Never edit the
old protocol or relabel its sparse evaluation history as a four-batch run.

## Canonical metrics

Reuse the established harness keys and axis names wherever the definition agrees:

- `train/actions`, `train/batches`, `time/elapsed_seconds`.
- `evaluation/all_admitted/by_{actions,batches,seconds}/area_reduction_percent`
  and `/delta_from_initial_pp`, measured against matched initial evaluation cases.
- `evaluation/accepted`, `/attempted`, `/coverage_fraction`, `/paired_cases`,
  `/failed_actions`, `/failure_fraction`.
- Root `ppo/value_loss`, `/approximate_kl`, `/outside_clip_fraction`, `/value_mae`,
  `/epochs`, `/actor_epochs`, `/critic_epochs`, `/raw_advantage_std`.
- `timing/collection_seconds`, `/update_seconds`; genuine experiment-specific
  success/buffer metrics retain their own names.

Persist elapsed training seconds with every new update instead of substituting
W&B upload time. Area-reduction quality is a checkpoint evaluation measurement,
not the sum of training rewards. Do not assign old names to different quantities,
fill unavailable metrics with zero, or claim the original cohort was evaluated
unless those exact cases were executed. Common training episode metrics require
completed episode evidence; omit them on unfinished chunks.

Normal detached jobs must start and supervise W&B tracking automatically when
online tracking is configured. Readiness runs may explicitly disable tracking.
Configuration and tracking failures must be visible rather than silently creating
an untracked long run.

Preserve the stopped v1 run as historical evidence with its actual sparse
six-case evaluation. The fresh v2 run uses the canonical keys from iteration zero;
do not mix the old cohort into the new series or reuse the old observer cursor.
Verify canonical metrics from server history and the existing chart key contract.

## Verification

Check evaluation 0/4/final, native checkpoint continuation, sampler and model
invariance during evaluation, actual common metric values/custom axes, empty and
matched baselines, migration idempotence, and online initialization/termination.
Run repository checks and preserve compact provenance for the stopped job and
readiness results. Relaunch the corrected twelve-hour run with fresh initialization after verification.
