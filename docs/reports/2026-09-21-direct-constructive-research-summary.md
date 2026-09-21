# Direct constructive learning research summary

- Period: 2026-09-20 through 2026-09-21
- Scope: native constructive PPO, online full-graph training, route options,
  successful-experience replay and success-adaptive frontier training
- Current decision: keep the research infrastructure, but do not integrate the
  learned route option into full-factory construction yet

## Executive result

The project now has a complete, reproducible learning path from external game
catalogs to native constructive actions, batched PPO updates, held-out
evaluation and retained CUDA evidence. Repeated experiments show a real learning
signal rather than a single-seed accident. The policy learns pairwise tasks,
benefits from recipe diversity and improves early progress on full production
graphs.

The unresolved problem is narrower than “the model cannot route.” The policy can
construct a crossed first route. From a saved, locally actionable first-route
state it can also discover many valid second routes. It still cannot reliably
produce a compatible first route and then continue with the second route when
started from a fresh task root.

In ordinary terms, the learner can practice the second half of the problem when
placed at the halfway point, but it cannot yet join the first and second halves
into one dependable skill.

## Research progression

| Stage | Question | Main evidence | Decision |
| --- | --- | --- | --- |
| Native constructive pilot | Can PPO learn legal constructive commands at all? | End-to-end collection, update and checkpoint paths worked | Replicate across seeds before scaling |
| Five-seed replication | Was the stochastic gain a seed accident? | Held-out stochastic completion rose from 63/1,280 to 219/1,280; every seed improved | The learning signal is real; broaden the task distribution |
| Recipe diversity and CUDA | Does broader data still learn, and can it run efficiently? | Three-seed stochastic completion rose from 54/1,536 to 354/1,536; encoder reuse improved throughput by 1.39x and reduced peak VRAM | Keep diverse external-data tasks and the optimized batched path |
| Online full graphs | Does direct PPO improve complete production graphs? | Reward improved in every seed, but final completed connections remained zero | Isolate routing instead of spending more on long full-graph runs |
| Corrected route options | Can a smaller route skill cross the routing cliff? | Aligned greedy routing reached 240/240 pooled; crossed successes occurred only at route depth zero | Preserve the option harness, but diagnose the first crossed continuation |
| Successful replay | Can retaining good routes solve the missing continuation? | Pooled crossed greedy success improved from 8/96 to 16/96, but all gains were single-route depth-zero cases | Replay preserves known skills but does not create the missing depth-one experience |
| Frozen frontier probe | Is a valid crossed second route representable and discoverable? | 49 valid depth-one continuations; temperature 1.5 produced 18/256 actionable successes and 17 unique command paths | Keep the representation and train directly at the frontier |
| Adaptive frontier training | Does focused frontier practice transfer back to fresh roots? | 47 unique depth-one successes in 2,176 focused attempts, but fresh-root greedy depth-one success was 0/3 seeds | The remaining blocker is skill composition and state-distribution transfer |

## Confirmed results

### The learning and evaluation stack works

The repository now supports native constructive observation and action domains,
conditional attention-policy sampling, PPO and route-local SIL updates, strict
training/evaluation separation, deterministic checkpoints and CUDA execution.
Full-graph smoke tests traversed held-out graphs with up to 143 facilities, and
all retained treatment runs wrote complete reports and model snapshots.

The behavior likelihood remains exact when collection changes temperature. Each
sample stores its behavior temperature, and PPO recomputes every conditional
factor under the same transformed distribution. Mixed-temperature tests match
stored sampling log probabilities. Historical SIL labels deliberately use the
base policy distribution instead.

### The policy has a repeatable stochastic learning signal

The five-seed fixed-task replication and three-seed recipe-diverse experiment
both improved held-out stochastic completion in every seed. This rules out the
earlier concern that the first positive run was only lucky initialization.
Greedy behavior remained unstable, so stochastic improvement is evidence of a
better action distribution rather than evidence of a mastered deterministic
policy.

### Training performance is sufficient for bounded experiments

GPU sampling avoids transferring every packed factor distribution to the CPU.
Reusing encoded observations across PPO minibatches reduced matched update time
by 28.1% and peak VRAM from 2,102 MiB to 1,606 MiB. Full-graph training uses
bounded minibatches after a larger setting exhausted the 32 GiB GPU near the end
of a run.

### The route representation can express the missing continuation

The frozen frontier probe is the strongest evidence on representation. A
checkpoint that had never succeeded at crossed depth one during ordinary
training produced 49 valid depth-one routes when returned to its own actionable
first-route states. Successful paths appeared at every tested temperature and
across all four actionable prefixes. A route-local encoder may still improve
learning, but a representation replacement is not required merely to make a
solution possible.

### Focused practice works locally

The adaptive curriculum admitted only self-generated prefixes reconstructed
through a fresh native session. It rejected prefixes whose first route left no
legal next action. Seeds 101 and 202 reached the consolidation phase, and the
three runs produced 47 valid, semantically distinct depth-one successes. The
Wilson-bound controller kept collection within the fixed 64-session update
budget even when success was rare.

## Results that were ruled out

- More full-graph updates are not currently justified. Existing policies learn
  placement progress and then plateau without final connection completion.
- Ordinary successful-experience replay does not supply a missing route depth by
  itself. It can stabilize experience that collection already discovers.
- The original blocked-start explanation was too broad. Correct accounting
  separates a truly empty next option from an active route that later dead-ends.
- Port starvation is not the only explanation. Some self-generated first routes
  leave an actionable second route, and those states produce valid solutions.
- A decoder replacement is not the immediate next requirement. The current
  representation has already generated diverse valid depth-one routes.
- Higher exploration alone does not solve composition. Focused exploration
  creates local successes, but those successes did not transfer to fresh-root
  greedy execution.

## Remaining unknown

The evidence does not yet distinguish whether transfer fails because the
consolidation mix contains too few fresh two-route sessions, because the first
route changes faster than replayed frontier states, or because one shared policy
cannot retain both decisions under the current update pressure. These causes
make different predictions and should be tested before changing the model.

The smallest useful next experiment changes only the consolidation supply:

1. Keep temperature-1.5 restored-prefix practice until the first diverse
   depth-one successes appear.
2. During consolidation, allocate most of that budget to fresh two-route crossed
   tasks, so the policy must produce the first route and continue immediately.
3. Retain a smaller restored-prefix stream as rehearsal so the second-route skill
   does not disappear.
4. Keep the total 64-session budget, PPO/SIL objectives, model, held-out panel and
   three initialization seeds unchanged.
5. Pass only if at least two seeds produce a fresh-root greedy
   `crossed-depth-1` success.

If this paired consolidation still produces local frontier success without
fresh-root transfer, compare a route-local encoder or separate option parameters
on the same fixed boundary. Do not add solver-generated demonstrations,
unbounded retries or full-factory integration before that test.

## Durable research assets

The work leaves reusable infrastructure even though the final gate has not
passed:

- externally generated pairwise and full-graph task distributions;
- online balanced full-graph generation with a fixed held-out panel;
- native self-roll-in route collection with corrected dead-end accounting;
- bounded, causal, diversity-aware successful-experience replay;
- frozen frontier probing with semantic prefix deduplication;
- exact temperature-aware PPO likelihoods;
- probability-budgeted discovery, consolidation and mastery control;
- joint task-shape and roll-in-depth metrics that expose hidden frontier failure;
- retained multi-seed CUDA reports, configs and model snapshots.

The detailed evidence remains in:

- [`2026-09-20-direct-constructive-replication.md`](2026-09-20-direct-constructive-replication.md)
- [`2026-09-20-direct-constructive-gpu-and-recipe-diversity.md`](2026-09-20-direct-constructive-gpu-and-recipe-diversity.md)
- [`2026-09-20-direct-constructive-full-graph-online-three-seed.md`](2026-09-20-direct-constructive-full-graph-online-three-seed.md)
- [`2026-09-20-self-roll-in-route-options.md`](2026-09-20-self-roll-in-route-options.md)
- [`2026-09-20-route-frontier-temperature-probe.md`](2026-09-20-route-frontier-temperature-probe.md)
- [`2026-09-20-success-adaptive-route-frontier-training.md`](2026-09-20-success-adaptive-route-frontier-training.md)

## Commit lineage

| Commit | Contribution |
| --- | --- |
| `3b6a993` | Direct constructive PPO pilot |
| `fbea088` | Recipe-diverse CUDA training |
| `c06165c` | Constructive encoder reuse |
| `3c8b8a6` | Prepared full production graphs |
| `284ee89` | Online full-graph sampling and training |
| `5e9fc96` | Self-roll-in route option training |
| `0f17196` | Successful route-option replay |
| `b831500` | Frozen frontier temperature probe |
| `c9db0d8` | Success-adaptive route frontier training |
