# Blueprint width and message-depth capacity screen

Increasing width or message depth did not produce a consistent quality gain in
this 512-batch screen. The width-40/two-round baseline had the highest two-seed
final and late-window means, but its final advantage came entirely from seed 17.
Width 80/two rounds performed better on seed 29. These results do not justify
scaling the current model as a reliable solution to the harder blueprint contract.

All eight arms completed their matched budgets, final physical/learner audits
and online tracking. The server service exited successfully after 83.55 minutes.

## Quality results

All values below are mean validated area reduction percentages over the same
32 factories; larger is better. The late window contains evaluations 388–512.

| Width / rounds | Final, seed 17 | Final, seed 29 | Final, two-seed mean | Late-window, two-seed mean |
|---|---:|---:|---:|---:|
| 40 / 2 | 1.9525% | 0.0000% | **0.9762%** | **0.4185%** |
| 80 / 2 | 0.0000% | 0.2983% | 0.1491% | 0.2956% |
| 40 / 4 | 0.0000% | 0.0000% | 0.0000% | 0.1199% |
| 80 / 4 | 0.0000% | 0.0000% | 0.0000% | 0.0083% |

The baseline's late-window means were 0.8020% and 0.0349% by seed; width 80/two
rounds achieved 0.2477% and 0.3434%. Thus the ranking also reverses by seed in
the late window. The baseline's two-seed untrained mean was 0.7384%, above its
late-window mean, although below its final mean. This is not evidence of reliable
learning gains or convergence even for the baseline.

Changes from each architecture's own untrained evaluation, averaged over seeds,
were +0.2378, -0.7455, -0.7384 and -0.8166 percentage points in table order.
Against the same-seed baseline, final paired area wins/ties/losses across 64
factory-seed pairs were 2/56/6 for width 80/two rounds and 0/58/6 for both
four-round models. The baseline improved six factories in seed 17; width 80/two
rounds improved two in seed 29. The other final evaluations retained original
area. Zero area reduction does not imply an invalid retained factory.

Training success did not translate consistently into separate-factory quality:

| Width / rounds | Late training area reduction, seed 17 / 29 | Positive training commits, seed 17 / 29 |
|---|---:|---:|
| 40 / 2 | 3.7598% / 3.4180% | 453 / 388 |
| 80 / 2 | 6.8555% / 0.0000% | 907 / 23 |
| 40 / 4 | 0.1660% / 3.1836% | 18 / 368 |
| 80 / 4 | 0.5957% / 6.3672% | 77 / 811 |

For example, the largest model's seed 29 improved the training fixture by a
late-window mean of 6.3672% yet ended with zero mean area reduction on the
evaluation cohort. Conversely, width 80/two rounds had no late training area
gain in seed 29 but a small final evaluation gain. Capacity, training success
and evaluation quality cannot be treated as interchangeable measures here.
This comparison does not isolate the cause of the mismatch.

## Measured cost

These are two-seed means at the same action budget. Inference covers a complete
64-action evaluation case, including the native environment and graph preparation.

| Width / rounds | Active minutes per arm | Optimizer seconds per arm | Mean evaluation inference per case |
|---|---:|---:|---:|
| 40 / 2 | 36.97 | 211.15 | 255.43 ms |
| 80 / 2 | 37.68 | 418.16 | 233.30 ms |
| 40 / 4 | 34.90 | 273.97 | 232.09 ms |
| 80 / 4 | 41.87 | 602.19 | 247.37 ms |

Optimizer time rose about 1.98 times for width 80/two rounds and 2.85 times for
width 80/four rounds. Whole-run and evaluation timings also depend on chosen
actions, route work, success replay and serialization, so they do not measure
architecture speed in isolation. All 33,024 evaluation cases reached 64 actions;
none hit the elapsed guard. The eight arms completed 16,384 training episodes.
Main outputs occupy 40.62 GiB, leaving 13.23 GiB free on the server, above the
registered eight-GiB reserve. The temporary tracking environment file was removed
after all jobs and observers finished.

## Context

The user requested testing greater model capacity after the blueprint-edit
contract became harder. The monitored Mac trainer was explicitly stopped at
1,396 batches / 357,376 training actions, its final audit passed, and its launch
registration was removed. The server's older padded-curriculum trainer was
already stopped. This experiment imports no previous learner or success buffer.

## Registered comparison

The [design](../designs/2026-09-11-blueprint-model-capacity-screen.md) and
`tools/factory-edit-pilot/blueprint-capacity-screen.json` were committed before
the main matrix started. Cross hidden widths 40/80 with two/four message rounds;
keep the graph input, actor/critic ownership, message/update MLPs, candidates,
reward, GAE, advantage normalization, learning rate, KL stopping and success
replay coefficient unchanged. No per-architecture hyperparameter tuning occurs.

| Model | Parameters | Training seeds | Batches per seed | Actions per seed |
|---|---:|---|---:|---:|
| Width 40, two rounds | 26,842 | 17, 29 | 512 | 131,072 |
| Width 80, two rounds | 101,682 | 17, 29 | 512 | 131,072 |
| Width 40, four rounds | 43,002 | 17, 29 | 512 | 131,072 |
| Width 80, four rounds | 166,002 | 17, 29 | 512 | 131,072 |

Each batch contains eight 32-action chunks. Episodes cross a batch boundary and
terminate after 64 actions, yielding 2,048 complete training episodes per arm.
The matrix budget is 1,048,576 training actions. All learners start from fresh
weights, empty Adam states and empty success buffers.

Evaluate the authentic 32 historical factories before learning, every four
batches, and at batch 512: 129 evaluation points per arm. Every factory retains
its original request, initial layout, action seed, 64-action cap and two-second
native elapsed guard. Primary comparisons use the final mean validated area
reduction and the predeclared last-128-batch window, evaluations 388 through 512.
Results are descriptive, separated by learner seed; no peak checkpoint is selected.

## Execution and verification

All eight arms use the same Intel Core Ultra 9 285K server, Linux native release
library, Python 3.12.13 and Torch 2.14.0+cu130 on CPU with one Torch thread.
Up to four workers run on explicitly pinned CPU slots 0, 1, 4 and 5. The seed-17
models launch in matrix order; seed-29 models launch in reverse order as slots
become available. Actual per-arm CPU assignments remain in matrix state.
This is observed pipeline cost under the registered schedule, not an isolated
neural-kernel speed benchmark. Active time includes collection, optimization,
checkpoint writing and evaluation. Final audit and tracking shutdown are
recorded separately in completion timestamps.

Runtime sources are frozen at `cd2f640`. The default and explicit two-round
models initialize identically. Native regressions verify packed/single outputs,
gradients through every message round, and exact restoration of a larger/deeper
learner with nonempty Adam state. All eight readiness arms completed eight
updates and passed their final audits. A full audit of the width-80/four-round
readiness arm reproduced all eight optimizer updates, validated 32 training
episodes and checked 96 evaluation cases. All 768 readiness evaluation cases
completed 64 actions.

The Python suite ran 417 tests: 410 passed and seven skipped. Rust workspace
checking passed; workspace tests passed 564 tests with five ignored. The analysis
CLI was exercised on the complete readiness matrix. It rejects mismatched
non-factor configuration, sources, native builds, input identities, evaluation
cadence, child protocol bindings or final audit/tracking records. Paired-case
regressions reject altered sampling seeds and mismatched cohort identities.

The W&B view is scoped to this matrix's exact specification hash. It overlays
canonical evaluation metrics against training batches, actions and elapsed time,
along with training quality, success replay and compute costs. Saved panel and
filter definitions are read back; iteration-zero history is verified against
the actual online runs. All 1,032 online evaluation points are compared against
their durable area reduction, paired baseline delta and training action count.
The readback explicitly distinguishes null evaluation fields on training rows
from actual zero-valued evaluation points.

## Interpretation limits

This is a fixed-budget early capacity screen, not a convergence experiment or
an optimal architecture search. Two learner seeds and one fixed evaluation
sampling seed per factory do not establish a population-level ranking. The last
32 evaluations are repeated measurements during training, not 32 independent
training seeds.

Training still repeats one five-facility fixture in original and overlap starts;
evaluation uses distinct factories. Greater training success alone cannot show
better layout generalization. The existing observation includes elapsed time,
so model-dependent inference cost can also alter policy inputs even when every
case reaches the action cap. Keep these limits when interpreting width/depth
differences.

## Reproduction and retained evidence

Copy the registered specification to `target/NEW_CAPACITY_SPEC.json`, give it a
distinct name and preserve its other settings. Prepare a fresh output directory
with the frozen native library and authentic evaluation cohort, then launch the
explicit CPU worker slots:

```sh
python tools/factory-edit-pilot/blueprint_capacity.py prepare \
  --spec target/NEW_CAPACITY_SPEC.json \
  --native-library target/release/libaic_pilot_native.so \
  --evaluation-cohort AUTHENTIC_COHORT_JSON \
  --output artifacts/factory-edit-pilot/NEW_CAPACITY_MATRIX \
  --env-file PRIVATE_TRACKING_ENV
python tools/factory-edit-pilot/blueprint_capacity.py run \
  --output artifacts/factory-edit-pilot/NEW_CAPACITY_MATRIX \
  --env-file PRIVATE_TRACKING_ENV --cpu-slots 0 1 4 5
python tools/factory-edit-pilot/compare_blueprint_capacity.py \
  --matrix artifacts/factory-edit-pilot/NEW_CAPACITY_MATRIX \
  --output artifacts/factory-edit-pilot/NEW_CAPACITY_MATRIX/analysis.json \
  --plot artifacts/factory-edit-pilot/NEW_CAPACITY_MATRIX/curves.png
```

A new replication should give its specification a distinct name, preserving
the experimental settings, so the W&B matrix filter does not mix replications.
Exact update replay uses retained observations and optimizer/RNG state; a fresh
seed alone does not reproduce wall-clock-dependent native trajectories.

Full checkpoints, chunks and evaluation witnesses remain on `ashrimp-arch` under
`/home/ashrimp/Srv/aic-blueprint-capacity-20260911/artifacts/factory-edit-pilot/`.
The exact selected Linux native library is also retained in `capacity-runtime/`,
outside Cargo cleanup, with its original path, build manifest and binary hash.
The local metadata and analysis directory is
`artifacts/factory-edit-pilot/blueprint-capacity-20260911/`. Generated evidence
stays ignored; only concise findings and compact provenance belong in Git.

The local `analysis.json` and `capacity-curves.png` retain the full curves;
`workspace-final.json` records online readback verification. The
[compact provenance](2026-09-11-blueprint-model-capacity-provenance.json) binds
these artifacts, matrix and child identities, the selected native library,
readiness audit, verification logs and the stopped prior run. The dedicated
[W&B view](https://wandb.ai/acrylicshrimp-personal/endfield-aic-layout-gen?nw=4dqqc53g6rr)
contains only this registered matrix.
