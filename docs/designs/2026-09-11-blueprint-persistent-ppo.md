# Persistent-draft blueprint PPO: bounded learning experiment

## Status and question

Registered on 2026-09-11 before implementation. The user requested execution of
the next learning milestone after the blueprint contract checks. Does a policy
trained with drafts preserved across rollout chunks recover exact original
connections and improve validated layouts during continuing inference?

This first learning experiment uses the existing five-facility crossing fixture.
It tests learning mechanics and within-fixture behavior, not generalization or
production readiness. Preserve `blueprint-edit-v1` topology/acceptance and the
new graph architecture. No old PPO observation/checkpoint adapter is introduced.

## Frozen experiment

Use fresh seeds 17 and 29, width 40 and the two-round blueprint graph
actor/critic. Keep learning rate `3e-4`, gamma 1, GAE lambda .95, clipping .2,
four epochs, minibatch 64, target KL .02, entropy .001 and gradient clip .5.
Joint updates train the shared encoder and actor with policy plus value loss;
the separate critic-head optimizer receives value gradients. After KL stopping,
freeze actor/encoder and continue only the critic head to four epochs. The new
architecture has no actor/critic history feature; prior model comparisons are not
treated as evidence that this architecture is optimal.

Sample from logits adjusted by the log number of candidates in each action kind,
so a large pose vocabulary does not get an accidental count advantage. Preserve
every declared candidate; do not mask overlap or preselect repairs/commits. Use
only native validated-best area reward, with no new failure or repair shaping.

Each seed receives 64 updates from eight live streams, 32 policy actions per
stream per update: 16,384 training actions per seed. A real episode has 256
policy actions, spanning eight updates. Chunk boundaries are not terminals.
The environment's action allowance additionally charges the explicit warm-start
prefix. Elapsed/routing bounds remain operational guards; premature resource
exhaustion fails the experiment with retained evidence.

Four equally represented starts use the external probe fixture and explicit
prefix actions: original, overlapping, repair-ready compaction, and already
validated compaction. Prefix actions are setup, not policy demonstrations or
training transitions. Record them and their costs. Report policy improvement
relative to the post-prefix best separately from total reduction relative to
the original blueprint. A planted compaction opportunity is not a policy-discovered
pose. Evaluation stratifies original, overlapping and already-improved starts;
the repair-ready training start is excluded from evaluation.

At updates 0/16/32/64 evaluate each stratum with two fixed sampling streams for
two seconds of native session budget, with a 4096-policy-action safety guard.
Record quality against elapsed time and at action milestones 8/32/64/128/256
when reached. Compare to the same seed's frozen untrained checkpoint. Native
session time charges observation, inference, action and prefix work; admission
is reported separately. These are diagnostic repeated cases, not an unseen test.

## Continuing credit and durable state

For each frozen behavior chunk, store pre-action model inputs, candidate/index
identity, log probability, value, reward and true termination. At a nonterminal
chunk boundary evaluate the successor using the same behavior checkpoint and
bootstrap `delta = reward + gamma * V(next) - V(current)`. Use zero successor
value only at a true episode terminal. GAE recursion stops at the sampled chunk
boundary, with the explicit bootstrap value. Freeze advantages and targets before
any minibatch update. Recompute sampled probabilities/values before updating.

Keep native streams alive across updates; assert blueprint, draft revision,
placements, paths, best and attempt continuity across each boundary. The clock
may advance during learner work. Store that difference instead of silently
resetting it. Wall time makes future replay nondeterministic; retain observations
and native snapshots rather than claiming that a seed reconstructs the stream.

`blueprint-edit-checkpoint-v1` snapshots include original/request/config, draft and
best paths/poses, hash, counters, validation status and charged elapsed time.
Restore revalidates original topology, all present paths and the best witness;
it cannot import a changed recipe, endpoint or false validated flag. Restored
clock time resumes from the recorded charge, excluding time the process was
offline. Old final-witness exports are not session checkpoints.

Learner checkpoints bind protocol/source hashes and retain model, both Adam
states, sampling/shuffle RNGs, active native snapshots and stream counters.
Write durable chunk evidence and checkpoints atomically. Retain non-replayable
inputs and selected checkpoints; never replace an interrupted stream with a
fresh environment under the same continuation identity.

## Verification and outputs

Before the main runs: test terminal versus chunk-boundary GAE numerically;
compare packed graph outputs/gradients with individual execution; verify actual
native continuation across an update; test checkpoint restoration and tamper
rejection; and reconstruct an update from a nonempty Adam checkpoint exactly.
Run a short native learner readiness job before the registered two-seed screen.

Reports expose per-start quality/time curves, improvements after actions 8/32,
valid commits, actual repairs versus restore-best, pending/overlap occupancy,
rejections, KL/clip/value diagnostics, bootstrap/terminal counts, continuity
checks, source/checkpoint hashes and termination reason. Every retained final
best and successful sampled commit must preserve the original topology and pass
full native validation. Sparse reward or no improvement is a research result;
do not change reward/curriculum mid-run to conceal it.
