# Lower PPO learning-rate screen

Keep 0.0003 for this model and 32-batch budget. Both lower rates finish worse
on both run seeds, despite substantially fewer KL stops and more actor optimizer
steps. Mean final area reduction is 58.45% at 0.0003, 55.83% at 0.0001 and
53.29% at 0.00003. This does not establish the best rate for longer training,
other architectures or independently tuned actor/critic rates.

All four new runs completed 128 batches and 256,256 training actions in
56.75 minutes. Native, checkpoint, input, source and remote tracking audits
passed. Training has ended; no further runs were launched.

## Final quality and learning curves

| Learning rate | Seed 17 | Seed 29 | Equal-seed mean | Mean difference from 0.0003 |
| --- | ---: | ---: | ---: | ---: |
| 0.0003, retained control | 55.97% | 60.94% | 58.45% | Reference |
| 0.0001 | 53.78% | 57.87% | 55.83% | -2.63 pp |
| 0.00003 | 50.95% | 55.62% | 53.29% | -5.17 pp |

The paired seed-17/seed-29 differences are -2.18/-3.07 pp at 0.0001 and
-5.02/-5.32 pp at 0.00003. The final ranking is the same on both seeds.
All three rates have identical initial quality: 41.30% for seed 17 and 48.83%
for seed 29, averaging 45.06%. Their mean improvements from initialization are
therefore +13.39, +10.76 and +8.22 pp in descending rate order.

Intermediate evaluations do not always follow that ranking. At batch 16,
0.0001 beats 0.0003 on seed 17 (49.87% versus 49.23%), while 0.00003 beats
0.0001 on seed 29 (56.18% versus 55.37%). Neither advantage survives to the
final evaluation. The lowest-rate seed-29 curve falls from 56.18% at batch 16
to 54.96% at batch 24 before recovering to 55.62%. A lower rate does not imply
a monotonic evaluation curve. All five evaluations for each seed/rate are
retained in the comparison report and the learning figure.

## KL behavior, optimization work and critic error

KL stops nearly disappear at the lower rates, but quality gets worse. The
following epoch and KL means first average the 32 batches within each seed,
then average the two seeds equally. Step counts and stopped-batch counts are
totals across both runs.

| Rate | KL-stopped batches / 64 | Mean actor epochs / batch | Actor optimizer steps | Mean first-epoch KL | Mean final-epoch KL |
| --- | ---: | ---: | ---: | ---: | ---: |
| 0.0003 | 38 | 2.80 | 5,642 | 0.01465 | 0.02152 |
| 0.0001 | 1 | 3.98 | 8,044 | 0.00656 | 0.01121 |
| 0.00003 | 0 | 4.00 | 8,076 | 0.000484 | 0.00359 |

Actor steps increase by 42.6% and 43.1% at the lower rates. The only lower-rate
early stop is the final batch of seed 17 at 0.0001, after three actor epochs.
Critic work remains four nominal epochs per batch and 8,076 optimizer steps
per rate. However, more of those epochs are joint encoder updates when actor
stopping disappears, rather than private-head-only tail updates.

| Rate | Mean final outside-range fraction | Mean objective-clipped fraction | Final late critic MAE | Final failed-action fraction |
| --- | ---: | ---: | ---: | ---: |
| 0.0003 | 26.48% | 18.61% | 1.51 pp | 15.76% |
| 0.0001 | 15.85% | 11.72% | 2.03 pp | 14.60% |
| 0.00003 | 2.62% | 1.86% | 3.17 pp | 11.02% |

Clipping fractions are measured after each batch's final actor epoch and then
averaged across batches and seeds. Final critic error and failed-action fraction
are final-evaluation measurements. Lower clipping and fewer failed selected
actions did not translate into better final layouts. All accepted final layouts
are valid; failed-action fraction describes proposals rejected during editing.
Critic MAE gets worse on both seeds at both lower rates, but these errors follow
each policy's own trajectories and cannot isolate a critic-only learning-rate
effect.

| Rate | Mean training-phase seconds / run | Mean PPO-update seconds / run |
| --- | ---: | ---: |
| 0.0003 | 712.11 | 459.83 |
| 0.0001 | 780.48 | 529.99 |
| 0.00003 | 804.48 | 545.93 |

Every rate selects 64,128 training actions for seed 17 and 64,000 for seed 29.
Training-phase time includes setup, collection, optimization and checkpoint
saving. Matrix wall time additionally includes evaluation, startup and export.
The lower rates take 68.37 and 92.37 more training seconds per run on average;
their smaller steps do not make this fixed-experience experiment faster.
Timing remains descriptive because controls were run earlier.

## Controlled comparison

The user requested lower learning rates and explicitly excluded higher rates.
Compare fixed Adam learning rates 0.0001 and 0.00003 against the retained
0.0003 control. Both actor and critic rates change together. Fix the shared
width-40 model with two CNN layers, critic history and critic tails after KL
stopping. The actor observes 16 past actions and cannot observe the best world.
This is the best wider-shared condition in the preceding capacity screen,
selected before this learning-rate screen began.

Each new rate uses seeds 17 and 29, 32 batches of 64 factory attempts, and
32 actions per admitted factory. All runs start from fresh, matching parameters.
Evaluate at batches 0, 8, 16, 24 and 32 on the same 32 attempted factories per
seed. The retained 0.0003 runs have the same protocol. No learning-rate schedule,
warmup, changed KL threshold, extra epochs or inherited-start curriculum is used.

The outcome is mean per-factory percentage reduction in best valid used area
relative to the initial factory. Higher is better. Average within seed first,
then weight the two seeds equally; seed 17 admits 32 evaluation factories and
seed 29 admits 30. Paired differences are percentage points.

The maximum is four PPO epochs per batch. Policy KL is checked after each full
epoch against 0.02; the reported early-stop count includes batches completing
fewer than four actor epochs. A last-epoch KL crossing does not count as an early
stop because it saves no epoch. Critic tails complete the remaining epochs while
keeping the shared encoders, actor and actor optimizer frozen. Lower rates can
therefore increase actor update work even when collected experience is fixed.
For the shared model, fewer stops also replace some private-head-only critic
fitting with additional joint encoder updates, even though nominal critic epochs
remain four. This is a consequence of changing rate under the fixed KL schedule.

Late critic MAE covers actions 9 through 32 against the remaining realized
reward sum (gamma 1). It is measured before fitting that episode. First-epoch
KL and clipping fractions are evaluated on the collected behavior samples.
Outside-clip-range fraction and objective-clipped fraction are different
diagnostics; the latter depends on the advantage sign.

## Interpretation limits

Two seeds are exploratory, and the reused held-out factories are not an
untouched confirmation set. The seed controls initialization and factory/action
draws, so compare treatments within a seed. This screen is conditional on the
selected architecture and schedule; it does not establish a learning-rate
ranking for separate encoders or other critic settings.

Both learners' rates change, so actor and critic effects cannot be separated.
Fewer KL stops do not establish better optimization or final quality. Equal
experience does not imply equal optimizer steps. Critic errors follow each
policy's own trajectories and return distributions, not common regression
targets across trained models. Retained controls were run earlier, and wall-clock
differences also reflect visitation, routing and machine conditions.

## Reproduction and evidence

The final native audit validated all 124 admitted final factories across 128
attempts, with the same admission pattern as the controls. Every admitted
evaluation episode completed all 32 actions. Retained trace reward, prediction
and failure accounting, checkpoint chains, optimizer-step counts and critic-tail
actor freezes passed the common comparison audit.

The combined comparison verified 60,793 file identities, including all 40,434
files referenced by the retained capacity audit and 20,354 in the new rate
audit. All 4,416 scheduled generation positions have matching admission,
request, initial-layout and geometry identities. At all three rates, both seeds
have identical initial actor/critic hashes and identical initial evaluation
metrics. Initial evaluation plus first-batch collection also matches at all 192
positions, including 5,888 native behavior steps: actions, masks, observation
fingerprints when present, probabilities, values, resulting states and rewards.
Optional trace field presence is preserved in this comparison; elapsed time is
excluded.

All 198 unique checkpoints across the three rates were read to verify both
Adam optimizers' learning rates and protocol/job/batch identities. This includes
the retained baseline checkpoints. New readiness separately reproduced one
second-batch update per rate from nonempty Adam state, with identical parameter,
optimizer, shuffle and epoch records, and validated all 96 readiness results.

All 217 fixed game/request/native/dependency identities match the retained
control. Of 55 Python runtime files, 54 are unchanged; only the W&B display label
adds learning rate. PPO configuration differs solely in learning rate, and model
configuration is identical. The same Linux environment uses Python 3.12.13,
PyTorch 2.14.0, NumPy 2.5.3, W&B 0.29.0 and CUDA 13.0.

W&B confirms all four new runs finished, with matching model and full PPO
configuration, 168 history rows and 16,142 numeric scalars. Final Linux/CUDA
regression passed all 95 tests. Local regression passed 92 tests with three
platform-specific skips. Workspace check and tests, whitespace checks, and
worktree/staged artifact-policy checks passed. Both generated figures were
visually inspected.

The [compact provenance](2026-09-11-ppo-learning-rate-provenance.json) records
source hashes, reports, external full manifests, checkpoint coverage and figures.
Local report/figure copies are under
`artifacts/factory-edit-pilot/ppo-learning-rate-20260911/`. Learning and update
figures are `plots/learning.png` and `plots/updates.png` respectively.

The [registered contract](../designs/2026-09-11-ppo-learning-rate-screen.md)
declares the rates, controls, readiness gate and evidence requirements.
The [tool README](../../tools/factory-edit-pilot/README.md#lower-learning-rates-for-the-wider-shared-model)
provides preparation, execution, native audit, W&B verification, paired comparison
and plotting commands.

Main source is frozen at commit `5d3ed19` under
`/home/ashrimp/Srv/aic-ppo-learning-rate-screen-20260911`.
Analysis runs separately under
`/home/ashrimp/Srv/aic-ppo-learning-rate-analysis-20260911`.
The old width-40 screen and its checkpoints remain unchanged. Raw outputs,
checkpoints, full manifests and figures stay under ignored artifacts directories.
