# Shared PPO capacity screen

Increasing shared width improves mean final area reduction in all four
history/schedule conditions, by 0.08 to 1.57 percentage points versus shared
width 32. The average improvement is 0.67 pp; individual paired runs improve in
six of eight cases. The wider shared model still trails separate width 32 by
0.81 pp averaged across conditions. With critic history it matches or exceeds
the separate controls on the two-seed means; without history it remains worse
by 1.79 to 2.38 pp. Capacity helps but does not remove the conditional advantage
of separate encoders.

All eight runs completed 32 batches, totaling 512,512 training actions in
95.98 minutes. All native, provenance, checkpoint, schedule and remote tracking
audits passed. No additional training was launched.

## Final quality

| Critic history | Schedule | Shared 32 | Shared 40 | Separate 32 | Wider minus shared 32 | Wider minus separate 32 |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| Off | Joint stop | 54.64% | 56.21% | 58.00% | +1.57 pp | -1.79 pp |
| Off | Critic tail | 56.22% | 56.30% | 58.68% | +0.08 pp | -2.38 pp |
| On | Joint stop | 57.78% | 57.98% | 57.72% | +0.20 pp | +0.25 pp |
| On | Critic tail | 57.61% | 58.45% | 57.77% | +0.84 pp | +0.69 pp |

The largest wider-versus-small-shared mean gain, without history and with joint
stopping, is driven mainly by seed 29. The near-zero no-history/tail mean hides
opposite signs. Neither should be summarized as a uniform capacity benefit.

| Critic history / schedule | Wider minus shared 32, seed 17 / 29 | Wider minus separate 32, seed 17 / 29 |
| --- | ---: | ---: |
| Off / joint stop | +0.06 / +3.09 pp | -2.48 / -1.09 pp |
| Off / critic tail | +1.21 / -1.05 pp | -1.20 / -3.56 pp |
| On / joint stop | -0.38 / +0.77 pp | +0.59 / -0.08 pp |
| On / critic tail | +0.50 / +1.19 pp | +1.33 / +0.04 pp |

Within the wider model, enabling critic history improves mean final area
reduction by 1.96 pp averaged across schedules and seeds. Critic tails add
0.28 pp averaged across histories. Both tail comparisons have opposite signs
across the two seeds: +1.69/-1.51 pp without history and +1.07/-0.11 pp with
history. The history-by-tail difference of differences is +0.39 pp. These are
descriptive factorial contrasts, not significance tests or independent repeats.

## Starting policies and learning

Width 40 starts lower on both seeds: 41.30% versus 42.56% for seed 17, and
48.83% versus 51.53% for seed 29. The mean initial deficit is 1.98 pp. Both
width-32 architectures share the same initial actor within each seed.

| Critic history / schedule | Gain from initialization, shared 32 | Gain, shared 40 | Gain, separate 32 |
| --- | ---: | ---: | ---: |
| Off / joint stop | +7.59 pp | +11.15 pp | +10.95 pp |
| Off / critic tail | +9.18 pp | +11.24 pp | +11.64 pp |
| On / joint stop | +10.73 pp | +12.91 pp | +10.67 pp |
| On / critic tail | +10.56 pp | +13.39 pp | +10.72 pp |

Wider-minus-shared-32 gain averages +2.65 pp, while its final advantage is only
+0.67 pp. Relative to separate width 32, gain averages +1.17 pp despite final
quality being -0.81 pp. These differences illustrate why learning gain and final
level must be distinguished; gains from unequal initial functions do not
establish faster causal learning. Every condition/seed curve at all five
evaluation points is retained in `plots/learning.png`.

## Critic diagnostics and work

Late critic MAE covers actions 9 through 32, against the remaining realized
reward sum, in percentage-point units. Lower is better on that trajectory.

| Critic history / schedule | Late MAE, shared 32 | Shared 40 | Separate 32 | Wider final action failure rate |
| --- | ---: | ---: | ---: | ---: |
| Off / joint stop | 3.38 | 2.80 | 1.50 | 10.69% |
| Off / critic tail | 2.68 | 2.39 | 1.54 | 12.26% |
| On / joint stop | 1.65 | 1.63 | 1.47 | 20.22% |
| On / critic tail | 1.67 | 1.51 | 1.54 | 15.76% |

Widening lowers mean late critic error versus the small shared model in all four
conditions. However, without history the wider critic remains substantially
worse than separate encoders on their respective trajectories. Failed selected
actions are not invalid accepted final factories. Failure rate reflects policy
selection and does not rank final area quality by itself.

Every run selects 64,128 training actions for seed 17 and 64,000 for seed 29,
identical to its controls. Each column below averages the two seeds.

| Critic history / schedule | Training seconds, shared 32 | Shared 40 | Separate 32 | Wider actor / critic epochs per batch |
| --- | ---: | ---: | ---: | ---: |
| Off / joint stop | 557.68 | 605.00 | 603.98 | 2.81 / 2.81 |
| Off / critic tail | 599.25 | 735.74 | 678.55 | 3.03 / 4.00 |
| On / joint stop | 519.02 | 595.86 | 601.07 | 2.64 / 2.64 |
| On / critic tail | 574.93 | 712.11 | 683.79 | 2.80 / 4.00 |

Training seconds sum measured training-phase wall time, including setup,
collection, PPO updates and checkpoint saving. The matrix's 95.98 minutes
additionally includes evaluation, startup and export. Width 40 takes
99.46 more training seconds per run than shared width 32 on average, including
77.18 more update seconds, despite averaging 0.30 fewer actor epochs per batch
and 300 fewer actor optimizer steps per run. KL stops occur in 6.13 more batches
per run on average. Thus equal experience does not imply equal optimization
work, and larger-model optimization remains a possible follow-up question.
These timings do not isolate hardware throughput or a causal width cost.

## Comparison contract

The shared model increases from width 32 to 40: 58,530 to 86,282 unique
parameters, compared with 93,090 for the separate width-32 actor/critic pair.
This is 47.41% more than the old shared model and 7.31% fewer than the separate
pair. CNN depth stays at two layers. Graph depth, normalization, action inputs,
reward, PPO configuration and native execution remain fixed.

Run all four combinations of critic history and critic fitting after policy KL
stopping, with seeds 17 and 29. The actor always observes its last 16 actions;
the history factor changes only critic visibility. A shared-model critic tail
updates the private value head while the shared encoders and actor stay frozen.
The retained separate-model controls instead fit the entire private critic.

Each fresh run trains for 32 batches of 64 factory attempts, with 32 actions per
admitted factory. Evaluation uses 32 attempts at batches 0, 8, 16, 24 and 32.
Both previous width-32 architectures use this same protocol. The earlier CNN
depth screen used 32 attempts per batch, so its per-run workload was about half
as large; this capacity screen does not increase the factorial's per-run budget.
The stopped twelve-hour inherited-start run remains stopped.

The outcome is the mean per-factory percentage reduction in best valid used
area from that factory's initial area. Higher is better. First average within
each run seed, then average the two seeds equally. Differences are percentage
points. Seed 17 admits 32 evaluation factories and seed 29 admits 30.

## Interpretation limits

These are two run seeds, each controlling model initialization and factory/action
draws. The four factor contexts are not extra independent replications. Retained
controls were run earlier, and the repeated held-out factories are not an
untouched confirmation set. Runtime differences also reflect changed KL
stopping, selected actions and machine conditions.

Width changes the actor's capacity and its initial function as well as the total
parameter count. Initial and final quality, and improvement from initialization,
must be shown separately. A larger gain from a weaker untrained policy does not
establish a causal learning-speed advantage. Approximately matching total
parameters does not match actor-only capacity, compute or initial policies, and
does not isolate actor/critic gradient interference.

Critic MAE is measured against realized remaining reward sums along each model's
own trajectories. Different return distributions and visited states can change
this diagnostic; it is not common-target regression across architectures.

The earlier four- and eight-layer CNN variants already used identity residual
additions for their extra layers (`x + SiLU(Conv(x))`). They were not standard
ResNet blocks, but their worse scores cannot be attributed simply to missing
skip connections. The current experiment changes width at fixed two-layer
depth and does not test alternative residual-block designs.

## Reproduction and evidence

The main comparison verified all 40,434 referenced file identities, recomputed
retained trace rewards and prediction metrics, checked checkpoint chains and
update accounting, and natively validated all 248 admitted final factories
across 256 attempts. All admitted evaluation episodes completed 32 actions.
Eight attempts were excluded by generation, consistently with the controls.

The cross-capacity audit verified 121,033 combined file identities, including
all 80,594 files referenced by the previous audited comparison. All 4,416
generation positions match across widths and cells, including failed admission,
request, initial-layout and geometry identities. Their canonical signature is
recorded in the compact provenance. Model initialization agrees within each
width across factor cells; cross-width parameter equality is not assumed.

All four stop/tail pairs agree exactly through their first tail: batch 6 for
no-history seed 17, batch 9 for no-history seed 29, and batch 8 for both
history-enabled seeds. This includes actor parameters and optimizer state after
the common update, as well as critic state before the first tail. Every recorded
tail preserves the actor/shared parameters, actor optimizer and joint shuffle.
The readiness audits separately replayed a nonempty-Adam update for both history
settings and verified all 96 native result records.

All 217 fixed data, request, native source/binary and dependency identities match
the retained controls. Among 55 Python runtime files, 53 are identical; the only
changes are the registered width specification and the W&B width label. Runtime
packages are identical: Python 3.12.13, PyTorch 2.14.0, NumPy 2.5.3, W&B 0.29.0,
CUDA 13.0, Linux. W&B confirms all eight runs finished, with 336 rows and 32,496
numeric scalars matching local records.

Verification completed:

- All 90 Linux/CUDA readiness regression tests passed before the main run.
- Final local PPO regression: 92 tests, 89 passed and three platform-specific
  native skips. The subsequent catalog-mismatch assertion passed both targeted
  capacity-analysis tests.
- `cargo check --workspace` and `cargo test --workspace` passed.
- All four analysis CLIs ran on the completed experiment; both generated figures
  were visually inspected. Original eight-cell effect calculations remain
  unchanged on the retained report, and tests cover reduced factor coverage,
  incomplete schedule pairs and mismatched inputs/configurations.
- Worktree and staged artifact-policy checks and whitespace validation passed.

The [compact provenance](2026-09-11-ppo-shared-capacity-provenance.json) records
the source hashes, matrix, reports, full external manifests, cells and figures.
Locally copied reports and figures are under
`artifacts/factory-edit-pilot/ppo-shared-capacity-20260911/`.

The workload and readiness gates were registered in the
[experiment contract](../designs/2026-09-11-ppo-shared-capacity-experiment.md).
The [tool README](../../tools/factory-edit-pilot/README.md#shared-model-with-approximately-matched-total-capacity)
documents preparation, execution, native comparison, remote tracking verification,
paired control comparison and plotting commands.

The frozen main runtime is
`/home/ashrimp/Srv/aic-ppo-shared-capacity-screen-20260911` at commit `6ae3c5c`.
Analysis runs separately in
`/home/ashrimp/Srv/aic-ppo-shared-capacity-analysis-20260911`; it does not modify
the frozen training source. Raw traces, checkpoints, input manifests and plots
remain under ignored artifacts directories. Commit only this report and compact
hash provenance with the analysis source.
