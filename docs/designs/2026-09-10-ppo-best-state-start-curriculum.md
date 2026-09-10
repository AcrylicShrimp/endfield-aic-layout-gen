# PPO best-state start curriculum

Status: implemented; four-batch native/CUDA readiness and exact PPO reconstruction passed.

The experiment mixes fresh factory starts with the per-factory best states from
the immediately preceding training batch. It retains the current CNN-2 model,
32-action horizon, PPO update algorithm, routing configuration and failure penalty. The
new twelve-hour run starts from the original seed-17 initialization; no learned
weights, optimizers, action histories or experiences are imported from the run
being replaced.

The user selected 64 factories per training batch (2,048 action slots), keeping
eight collection workers and the existing PPO minibatch/microbatch size. Evaluate
the same 32 fixed factories every four batches, approximately preserving the
previous experiment's action interval between evaluations. The fresh-input frozen
prefix covers 16 new batches, the same 1,024 scheduled task seeds as the previous
32-factory run's first 32 batches. Mixed inherited slots intentionally replace
some of those scheduled fresh inputs.

## Input and scheduling contract

The optional harness configuration `start_curriculum` has schema
`ppo-best-state-starts-v1`, `percentages: [80, 10, 10]`, and an explicit integer
`selection_seed`. This version implements exactly three depths:

- Depth 0: generate a fresh start with the task's scheduled generation seed.
- Depth 1: a uniformly selected depth-0 factory's best state from batch t-1.
- Depth 2: a depth-1 factory's best state from batch t-1, whose original fresh
  start therefore occurred in batch t-2.

Depth counts completed improvement passes, not necessarily positive area
decreases. Retain each factory's best state even when its latest pass cannot
improve it; do not filter out hard factories or rank parents by achieved area.
Record actual per-pass and root-relative area changes separately.

For N attempted episodes and batch b (one-based), the depth-1 quota is
`q(b) = floor(N*b/10) - floor(N*(b-1)/10)`. The depth-2 quota is `q(b-1)`.
The lag makes its required pool match the preceding batch's depth-1 pool.
At N=64, every five mature batches total 256/32/32 attempted episodes.
The first batch is all fresh; batch two has no depth-2 donors. Other donor
shortages are filled with explicitly reported fresh starts. Selection is
deterministic, without replacement, independent of scores and worker completion
order. Shuffle the scheduled depth assignments so groups do not occupy fixed
worker slots. Never use evaluation results as donors.

Task generation/action seeds remain unique scheduled task identities. Inherited
factory ancestry is recorded separately from those seeds. Fresh slots continue
using the existing frozen 1,024-task input prefix where applicable; all
standard evaluation factories remain frozen. Only initial factories are reused:
each new episode samples new actions with the current batch's immutable actor
and critic, resets episode memory/history, and enters the ordinary PPO update.

## Durable output and admission contract

Before committing a training phase, select and copy the next batch's required
donor request, original obligations and validated best geometry into a small
run-owned carry-forward bundle. Its manifest records source result/checkpoint
identities, parent batch/index/depth, root seed/input/area, expected best metrics,
per-file hashes, requested/actual slot counts and fallback reasons. Publish its
reference with the same committed phase record as the model checkpoint.

Resume must read that committed bundle and reproduce identical logical tasks.
Published task reuse remains conditional on matching task/actor/critic hashes.
Missing or corrupted parents/bundles are structured errors, never fresh-input
fallbacks. Bootstrap or a genuinely insufficient complete donor pool are the
only permitted fresh substitutions.

The worker copies inherited inputs into its own attempt directory and re-admits
them against the same complete request and native validator. The admitted
initial metrics/physical state must equal the parent's recorded best. A failed
admission is a structured curriculum error. Episode reward uses this admitted
start's area as its denominator; earlier passes' gains are not rewarded again.

Keep selected carry-forward bundles outside disposable generated/episode
directories. Ordinary post-commit retention may remove unselected training
outputs, but cannot remove the next batch's inputs or the selected ancestry
evidence. Uncommitted interrupted candidates never become donor inputs.

## Diagnostics and comparison

Use `ppo-harness.curriculum-*` diagnostic codes with severity, stage and entity.
Distinguish invalid configuration, cohort/parent identity, corrupt artifacts,
admission mismatch and resumed-plan mismatch. Report bootstrap/shortage
substitutions as explicit informational/warning diagnostics in the plan.

Phase records and W&B expose attempted/admitted counts by depth, additional
area reduction relative to each episode's own start, root-relative cumulative
reduction, action failure fraction, accepted physical-repeat fraction and area
contributions from the first eight versus remaining actions. Standard evaluation
remains the same fresh fixed cohort and is the primary comparison across runs.
Do not interpret the mixed training mean as an equivalent fresh-cohort score.

## Acceptance checks

- Validate deterministic quotas, startup/shortage behavior, no cross-job or
  evaluation parents, consecutive depth-2 ancestry and the depth ceiling.
- Verify carry-forward survival under normal cleanup, hash failures and exact
  logical-task reconstruction after interruption/resume.
- Verify reward conservation against the new episode start, per-depth metrics,
  current-policy experiences and unchanged standard evaluation inputs.
- Run bounded native/CUDA readiness through all three depths and independently
  reconstruct a PPO update containing depth-2 starts.
- Verify fresh initial actor/critic and empty Adam state against the original
  seed-17 initialization; stop the prior run only after readiness succeeds.
- Commit verified implementation, launch a new bounded twelve-hour run, confirm
  live progress and W&B metrics, and record launch provenance.
