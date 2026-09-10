# PPO actor/critic factorial results

Status: complete. All eight combinations and both seeds finished, followed by
native, checkpoint, input, tracking and mature-critic audits.

The highest final mean in this screen was **separate encoders, no critic history,
and continued critic fitting after policy KL stopping: 58.68% area reduction**.
Encoder sharing was harmful without critic history and approximately neutral
with it. Extra critic epochs had a modest average benefit, mostly in conditions
without critic history. Enabling every option was not the strongest combination.
These are screening results from two initialization seeds, not an established
general ranking or a change to production planner defaults.

## Scope and controls

The previous long run was [stopped after 258 batches](2026-09-11-padded-ppo-user-stop.md)
and preserved. The [registered design](../designs/2026-09-11-ppo-actor-critic-combinations.md)
compared the full shared/separate x critic-history/no-history x critic-tail/stop
matrix. Every actor retained its last-16-action input. A critic tail freezes all
actor parameters, actor Adam state and the joint shuffle stream; shared models
can consequently update only their private critic head during that tail.

Each of 16 fresh runs used 32 batches, 64 factory attempts per batch, and 32
actions per admitted factory. The matrix completed **1,025,024 training actions**
in **10,461 seconds (174.35 minutes)**. Per-seed training actions were identical
across treatments: 64,128 for seed 17 and 64,000 for seed 29. All source and native
inputs were frozen before execution under implementation commit `0d72382`.

Evaluation reused a fixed 32-attempt cohort at batches 0, 8, 16, 24 and 32.
Admission was identical across treatments: 32 factories for seed 17 and 30 for
seed 29. Initial area reductions were 42.56% and 51.53%, respectively. Effects
are paired within initialization seed before averaging, rather than pooling
factories or treating all factor contexts as independent runs.

This screen used fresh factory starts. It does not test the earlier 80/10/10
inherited-start curriculum, and its final percentages must not be directly
compared with the long run's different evaluation cohort.

## Final factory quality and value prediction

Area reduction is relative to each episode's initial bounding-box area. `Late`
is the additional reduction from actions 9–32, in percentage points of initial
area. Critic MAE uses realized remaining reward, including failure penalties,
and is shown in reward percentage points. The last three columns are equal
means of the two seed-level measurements.

| Encoders | Critic history | Critic schedule | Area %, seed 17 | Area %, seed 29 | Mean area % | Late gain pp | Late critic MAE pp |
| --- | --- | --- | ---: | ---: | ---: | ---: | ---: |
| Separate | No | Stop | 54.95 | 61.05 | 58.00 | 3.96 | 1.50 |
| Separate | No | Tail | 55.36 | 62.01 | 58.68 | 4.54 | 1.54 |
| Separate | Yes | Stop | 54.30 | 61.14 | 57.72 | 3.63 | 1.47 |
| Separate | Yes | Tail | 54.64 | 60.90 | 57.77 | 4.15 | 1.54 |
| Shared | No | Stop | 52.41 | 56.87 | 54.64 | 4.00 | 3.38 |
| Shared | No | Tail | 52.95 | 59.50 | 56.22 | 4.51 | 2.68 |
| Shared | Yes | Stop | 55.27 | 60.29 | 57.78 | 4.14 | 1.65 |
| Shared | Yes | Tail | 55.46 | 59.76 | 57.61 | 3.56 | 1.67 |

The [learning and cost figure](../../artifacts/factory-edit-pilot/ppo-combinations-20260911/plots/overview.png)
shows both seeds and the full evaluation curve. Seed 17's best individual result
was shared/history/tail, while seed 29's was separate/no-history/tail. A mean
ranking therefore does not imply that the same treatment won both seeds.

Later absolute gains are not a standalone measure of late-action quality: a
policy that moves improvements earlier can leave less easy progress after step
8. Likewise, lower critic MAE on another policy's trajectories can reflect
different visited states and return variance. The common-trajectory probe below
addresses the immediate predictive effect of extra fitting separately.

## Paired effects and interactions

Each main effect averages matched false-to-true differences within a seed.
Two-factor interactions are differences of conditional differences; the
three-factor entry repeats that difference once more. These are unscaled
differences, not regression coefficients under a +/-1 coding convention.

| Effect on final area reduction, pp | Seed 17 | Seed 29 | Mean |
| --- | ---: | ---: | ---: |
| Shared encoders | -0.790 | -2.170 | -1.480 |
| Critic history | +1.002 | +0.664 | +0.833 |
| Critic tail | +0.368 | +0.704 | +0.536 |
| Shared x history | +3.374 | +2.348 | +2.861 |
| Shared x tail | +0.001 | +0.681 | +0.341 |
| History x tail | -0.209 | -2.177 | -1.193 |
| Shared x history x tail | -0.272 | -1.965 | -1.119 |

The [effect figure](../../artifacts/factory-edit-pilot/ppo-combinations-20260911/plots/effects.png)
also shows later contributions and critic MAE. Lines span two seed observations;
they are not confidence intervals.

- Sharing changed area reduction by **-2.91 pp without critic history**, versus
  **-0.05 pp with history**, averaging both schedules and seeds. History largely
  removed the sharing penalty in this screen.
- Adding critic history changed area reduction by **+2.26 pp in shared models**
  and **-0.60 pp in separate models**, averaging both schedules and seeds.
  Its positive overall main effect should not be read as a universal benefit.
- Extra critic epochs changed area reduction by **+1.13 pp without history**
  and **-0.06 pp with history**. In the separate/no-history condition, the
  improvements were +0.404 and +0.961 pp. In the shared/no-history condition,
  they were +0.541 and +2.625 pp.
- On average, tails reduced failed-action fraction by 2.15 percentage points,
  but this also depended on the combination. Their average late critic MAE
  reduction was only 0.14 reward percentage points.

The mechanism behind the sharing/history interaction is not established by
these measurements. A plausible explanation is that the shared value gradient
benefits from observing the policy's history, but that is an inference requiring
a more direct gradient or representation experiment.

## Learning work and elapsed time

Epoch counts are means across all 64 updates in each two-seed cell. Times are
means per 32-batch run, including collection and updates but excluding evaluation
and startup. They are observed end-to-end costs, not isolated encoder benchmarks.

| Combination | Actor epochs | Critic epochs | Training minutes/run | KL-stopped updates / 64 |
| --- | ---: | ---: | ---: | ---: |
| Separate, no history, stop | 3.156 | 3.156 | 10.07 | 28 |
| Separate, no history, tail | 3.422 | 4.000 | 11.31 | 20 |
| Separate, history, stop | 3.109 | 3.109 | 10.02 | 32 |
| Separate, history, tail | 3.016 | 4.000 | 11.40 | 32 |
| Shared, no history, stop | 3.312 | 3.312 | 9.29 | 23 |
| Shared, no history, tail | 3.266 | 4.000 | 9.99 | 24 |
| Shared, history, stop | 2.984 | 2.984 | 8.65 | 30 |
| Shared, history, tail | 2.922 | 4.000 | 9.58 | 32 |

Sharing reduced unique parameter count from 93,090 to 58,530 (37.1%). Both
history conditions allocated the same history-capable critic to keep that factor
from changing parameter count. Shared models averaged 79 seconds less training
time per run; tails averaged 64 seconds more. Different KL stopping and visited
geometry also contribute to those elapsed-time differences.

## Mature-critic probe on identical trajectories

Retained batches 128, 192 and 256 of the stopped long run were reconstructed in
the original CUDA environment. Actor, critic, both Adam states, shuffle state and
all original epoch metrics matched exactly. Then the remaining critic epochs
were completed against the original frozen GAE targets, preserving the actor,
its optimizer and joint shuffle exactly.

For each fixed post-update actor, 32 new generation seeds produced 32 admitted
held-out trajectories. Before/after critics saw identical observations and
realized returns. These 32 seeds were disjoint from the parent's 16,544 generation
seeds; the held-out data never entered either fit. All 96 native episodes,
observation hashes, action masks, saved experiences and rewards passed validation.
The bounded probe finished in 77.55 seconds; its supervised process exited zero.

| Parent batch | Original actor epochs | Final critic epochs | All-position MAE, before -> after (pp) | Late MAE, before -> after (pp) | Training target loss change |
| --- | ---: | ---: | ---: | ---: | ---: |
| 128 | 1 | 4 | 0.9079 -> 0.8601 | 0.6438 -> 0.6382 | -33.77% |
| 192 | 2 | 4 | 1.2917 -> 0.8967 | 0.8217 -> 0.6592 | -67.49% |
| 256 | 1 | 4 | 0.7969 -> 0.8125 | 0.5517 -> 0.5613 | +0.39% |

The [position-by-position figure](../../artifacts/factory-edit-pilot/ppo-combinations-20260911/plots/mature-critic.png)
shows that these critics already tracked the sharp decline in remaining reward.
Batch 192 had appreciable underprediction that extra fitting reduced; batch 256
did not benefit by MAE, despite a smaller aggregate signed bias. Extra epochs
therefore sometimes address underfitting, but neither predictive error nor even
the final full-batch training loss is guaranteed to decrease.

These three snapshots share one training lineage and are not three independent
seeds. The probe measures value prediction, not subsequent actor improvement or
the causal source of limited later gains. Realized returns are stochastic
observations, not known conditional expectations. It does not establish that the
critic is the main remaining learning bottleneck.

## Verification and retained evidence

- The full comparator checked **80,594 file identities** and **4,416 matched
  input positions** across all eight cells, including runtime sources, native
  binaries, configurations, records, tasks, all checkpoints and retained traces
  and training experiences.
- All **512 final evaluation attempts** were audited: 496 complete layouts
  passed native initial/best validation, and 16 generator rejections matched
  the fixed admission results. No completed layout failed validation.
- All eight matched schedule pairs had identical joint updates through the
  first critic tail. First-tail batches for seeds 17/29 were 5/8 for separate
  no-history, 9/6 for separate history, 17/9 for shared no-history and 14/8 for
  shared history. Tail epoch/step counts and frozen-policy invariants passed.
- W&B independently confirmed all **16 finished runs, 672 rows and 65,047 numeric
  values**. One transient acknowledgement delay in shared/history/tail recovered;
  its stale local diagnostic was preserved, and the final remote data matched.
- Final `cargo check --workspace` and `cargo test --workspace` passed. The final
  Python PPO suite ran 88 tests: 85 passed and three platform-specific native
  tests were skipped.
  The earlier [CUDA/native readiness](2026-09-11-ppo-combinations-readiness.md)
  and this run's live native audits cover the Linux execution path.
- The three generated figures were inspected. Raw reports, traces, checkpoints
  and plots remain in ignored artifacts; no raw experiment dump is committed.

The [compact provenance manifest](2026-09-11-ppo-combinations-provenance.json)
retains hashes and locations instead of raw output. The original stopped run and
all selected inputs and checkpoints remain preserved. The matrix and probe
processes have finished; no further long training run was launched.

## Reproduction and interpretation boundary

The frozen matrix is at
`/home/ashrimp/Srv/aic-ppo-combinations-screen-20260911/artifacts/factory-edit-pilot/matrix`.
Analysis artifacts are under
`/home/ashrimp/Srv/aic-ppo-combinations-analysis-20260911/artifacts/factory-edit-pilot`.
Local reports and figures are under
`artifacts/factory-edit-pilot/ppo-combinations-20260911`.

Use the committed `ppo-actor-critic-screen.json` with
`run_ppo_combinations.py prepare` and `run` for a fresh matrix. Use new output
paths for each analysis invocation:

```sh
python tools/factory-edit-pilot/compare_ppo_combinations.py --matrix /path/to/matrix.json --output /fresh/comparison --validate-final-layouts
python tools/factory-edit-pilot/probe_ppo_critic_tail.py --run /path/to/stopped-run --output /fresh/mature-critic --batches 128 192 256 --attempts 32 --workers 8 --seconds 900
python tools/factory-edit-pilot/check_ppo_combinations_tracking.py --matrix /path/to/matrix.json --output /fresh/wandb-check.json --env-file /path/to/tracking.env
python tools/factory-edit-pilot/plot_ppo_combinations.py --report /path/to/comparison/report.json --critic-probe /path/to/mature-critic/report.json --output /fresh/figures
```

Exact historical replay requires the preserved runtime, native inputs and CUDA
environment. See the tool [README](../../tools/factory-edit-pilot/README.md) for
preparation and audit usage. Results support further testing of separate
encoders with critic continuation, while retaining history as an explicit factor.
They do not justify an unconditional sharing cutover, a universal winner, or
claiming that additional critic fitting solves later-action limitations.
