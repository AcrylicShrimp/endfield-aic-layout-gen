# Cold masked PPO comparison

Date: 2026-09-14. Status: complete. Four cold runs each reached update 128;
all 1,024 confirmation episodes completed, all 80 scientific worker PIDs exited,
and the main run remains paused at update 1908.

## Result

Training and executing with the structural band mask improved mean held-out area
reduction from **27.937% to 32.285%**, a **+4.348 percentage-point** change. The
paired geometry bootstrap interval is **[+2.263, +6.480] pp**.

The improvement includes a learning benefit: with masking enabled for both
evaluated models, masked training scores **32.285%**, versus **29.584%** for
original training. This **+2.701 pp** difference has interval
**[+1.365, +4.095] pp** and is positive for both initialization seeds.

| Training condition | Evaluation without mask | Evaluation with mask |
| --- | ---: | ---: |
| Original | 27.937% | 29.584% |
| Structural band mask | 28.363% | 32.285% |

| Paired comparison | Change pp | 95% interval pp | Geometry wins / losses / ties |
| --- | ---: | --- | --- |
| Full masked pipeline versus original pipeline | +4.348 | [+2.263, +6.480] | 23 / 7 / 2 |
| Masked training versus original training, both evaluated with mask | +2.701 | [+1.365, +4.095] | 22 / 9 / 1 |
| Evaluation-only mask on originally trained models | +1.647 | [-0.340, +3.640] | 18 / 13 / 1 |

The evaluation-only effect on these newly trained models and new geometries is
less certain than in the earlier frozen-policy screen. Its interval includes
zero. The favorable result here supports the masked learning pipeline within
this allocation; it does not establish that masking alone always helps.

## Replication and scope

| Initialization | Original train / original eval | Original train / masked eval | Masked train / original eval | Masked train / masked eval |
| --- | ---: | ---: | ---: | ---: |
| 71 | 27.390% | 28.896% | 27.173% | 31.815% |
| 113 | 28.484% | 30.272% | 29.552% | 32.756% |

The complete pipeline gains are +4.425 and +4.271 pp for seeds 71 and 113.
Additional learning gains under the same evaluation mask are +2.919 pp
([+0.930, +5.050]) and +2.484 pp ([+0.702, +4.373]) respectively.

Most of the additional learning benefit is on small factories:

| Factory size | Original / original | Original / masked | Masked / masked | Full pipeline change pp | Additional learning change pp |
| --- | ---: | ---: | ---: | ---: | ---: |
| Small | 33.844% | 35.860% | 41.591% | +7.746 | +5.731 |
| Large | 22.030% | 23.308% | 22.980% | +0.950 | -0.328 |

There is no demonstrated additional-learning improvement for large factories in
this aggregate. The registered bootstrap resamples whole geometries within the
two size strata, carrying both model seeds and all action draws together. The
intervals are conditional on these two initializations and four action draws;
they do not measure uncertainty over a broad population of independently trained
models. Per-seed intervals condition on their single observed initialization.

## Fixed protocol and input continuity

The pre-outcome design is
`../designs/2026-09-14-cold-masked-ppo-comparison.md`; the executable configuration
is `tools/factory-edit-pilot/operations/declarative-mask-training.json`.
Implementation commit: `05c01ff`.

- Four independent cold runs: original/masked for initialization seeds 71/113.
  Each pair starts with identical model tensors, empty Adam states and replay,
  identical initial sampler/shuffle RNG states, and no imported learning history.
- Every run receives 128 updates of 32 streams × 32 decisions: 131,072 decisions
  and 2,048 completed episodes per run; 524,288 decisions and 8,192 episodes total.
  The final update is used even when a development checkpoint scored higher.
- Preserve the accepted PPO, successful-experience replay, 80/10/10 start
  curriculum, native work limits, 64-decision episodes, and packed CUDA behavior
  collection with 16 persistent workers. Masking changes no model dimensions,
  root candidate indices, per-kind count normalization, or conditional port head.
- Generate new, separate development and confirmation panels, each containing
  16 small and 16 large unique geometries. Both exclude the earlier study's 64
  geometries, and all four panels are mutually disjoint. Training reserves their
  combined 128 identities; the retained input ledgers have no overlap.
- Reuse actual generated fresh inputs across arms through the existing canonical
  bank. There are 1,683 common fresh slots in the seed-71 pair and 1,691 in the
  seed-113 pair; all common payload, blueprint, request, geometry and generation
  identities match. All six cross-arm pairs also pass this check. Arm-boundary
  synchronization verifies the actual shared files by inode or content hash.
- Development evaluations use two action draws at updates 0/32/64/96/128. Final
  confirmation uses four draws on 32 new geometries for each of eight model/mask
  conditions. All receive 64 decisions and the original 10,000 ms native limit.

Policy-dependent inherited starts remain part of the experiment. The requested
depth counts are the same in every arm: 1,643 / 204 / 201 at depths 0/1/2.
Actual counts depend on donor availability and accepted inheritance:

| Arm | Fresh depth 0 | Inherited depth 1 | Inherited depth 2 |
| --- | ---: | ---: | ---: |
| 71 original | 1,713 | 201 | 134 |
| 71 masked | 1,705 | 201 | 142 |
| 113 original | 1,714 | 201 | 133 |
| 113 masked | 1,724 | 197 | 127 |

The experiment matches fresh inputs and curriculum rules, not all realized
trajectories or inherited layouts. It measures the effect of the learning
pipeline under those retained rules.

## Development curves

Values are mean physical area reduction on the separate development panel.
Each model uses its own training mask setting during development evaluation.

| Arm | Update 0 | 32 | 64 | 96 | 128 |
| --- | ---: | ---: | ---: | ---: | ---: |
| 71 original | 4.049% | 31.254% | 36.062% | 36.004% | 33.920% |
| 71 masked | 4.320% | 30.969% | 37.652% | 38.692% | 37.476% |
| 113 original | 2.682% | 20.834% | 25.851% | 35.052% | 35.165% |
| 113 masked | 3.831% | 33.374% | 39.536% | 40.803% | 42.460% |

The first seed shows little difference at update 32; the second original model
improves substantially between updates 64 and 96. These curves reinforce the
need to compare the fixed endpoints and both initializations. No early result
changed the configuration, allocation, or selected checkpoint.

## Native behavior and execution cost

All 1,024 confirmation best factories passed native validation. Every episode
ended at 64 decisions, with no resource termination. Model digests matched the
selected checkpoint in the coordinator and all workers after each measured
block. Both masked evaluation conditions have zero targeted structural band
rejections; the original pipeline wastes 8,445 of 16,384 decisions on them.
Other rejection modes remain available to the native validator.

| Mean per 64-decision episode | Original / original | Original / masked | Masked / masked |
| --- | ---: | ---: | ---: |
| All rejected actions | 33.730 | 1.551 | 1.367 |
| Placements | 16.223 | 38.621 | 37.891 |
| Delete-band actions | 45.652 | 22.238 | 20.156 |
| Insert-band actions | 2.125 | 3.141 | 5.953 |
| Improved outcomes, including non-area improvements | 9.895 | 10.547 | 10.848 |
| Post-action pending state | 26.734 | 48.984 | 50.504 |
| Post-action overlap state | 16.340 | 38.156 | 43.125 |
| Unfinished final drafts, out of 256 | 185 | 250 | 255 |

Improved valid-best output coexists with more unfinished drafts. Preparatory
placements, overlaps and pending routes remain permitted. Removing the mask
from masked-trained models lowers their mean from 32.285% to 28.363%; the mask
is part of their trained execution contract.

| Arm | Normal run seconds | Mean collection s/update | Mean learner s/update | Mean publication s/update | Retained training bytes |
| --- | ---: | ---: | ---: | ---: | ---: |
| 71 original | 1,490.28 | 5.297 | 3.903 | 1.683 | 682,900,299 |
| 71 masked | 1,471.95 | 4.339 | 3.512 | 2.923 | 1,252,954,023 |
| 113 masked | 1,511.47 | 4.689 | 3.480 | 2.913 | 837,261,591 |
| 113 original | 1,306.22 | 4.245 | 3.722 | 1.543 | 466,305,433 |

Mean run time is 1,398.25 seconds original and 1,491.71 seconds masked (+6.68%).
Publication costs and retained training data increase under masking. These are
observed integrated costs, not an isolated mask-overhead benchmark: sequential
arm order, fresh-input reuse, trajectories and replay retention also contribute.

The entire scientific section, including confirmation and analysis, completed
in **5,888.92 seconds (98.15 minutes)** within its 9,000-second allowance.
Confirmation blocks used 76.36 seconds of parallel wall time and 675.98 summed
worker CPU seconds. Actual distinct artifact size was **4,909,225,274 bytes**,
including readiness and panels, below the 12 GiB ceiling. The largest recorded
arm process-tree RSS was **27,802,738,688 bytes**; both inner and outer guards
enforced the 48 GiB limit. Every arm stayed below its 2 GiB retained-file limit.

## Verification and retained evidence

Ten targeted training-mask tests passed with the release native library,
including CUDA sampling/recomputed likelihood agreement, imitation, masked
gradients, original-path numerical continuity, and replay/restore behavior.
The shared frozen-mask suite also passed all five tests after the common-mask
cutover, including the exhaustive proposed-band check on the bounded fixture.
Workspace `cargo check` and `cargo test` passed.

The separate 137.71-second readiness run exercised real CUDA training and
successful replay, restored 64 live streams, and paused/resumed the masked arm
normally from update 2 to 4. Its source/config gate matched all scientific arms.
The final metadata audit took 0.445 seconds; it did not inflate checkpoints or
replay native episodes. It checked paired inputs, empty origins, curriculum
coverage, 8,192 accepted training completions, all confirmation outputs, worker
exit, and the paused main run. All 32 deployed operations Python files also
match the local implementation commit. No expensive audit ran in the training
loop.

The companion `2026-09-14-cold-masked-ppo-comparison.manifest.json` retains compact
outcomes, intervals, source/launch/checkpoint/panel bindings, initial-state
digests, development curves, input-pair checks and resource/closeout evidence.
Full report copies are under local ignored
`artifacts/cold-mask-training-20260914/`. Full native confirmation snapshots,
normal resume checkpoints and input banks remain under the research host root:

```text
/home/ashrimp/Srv/aic-band-mask-training-20260914/artifacts/
  readiness/
  panels/
  study/seed-{71,113}-{original,masked}/seed-{71,113}/checkpoints/
  study/confirmation/
  closeout.json
```

The isolated source root reuses the established environment and release
binaries. It imports no old learned state and does not modify earlier study
roots or resume the main run. The shared mask implementation replaces the
previous experiment-local classifier; normal training and frozen evaluation now
use the same structural rule.

For reproduction, run `cold_mask_study.py readiness`, then `panels`, then `study`
from the recorded source tree, with fresh output directories. The first two
commands take `--old-development` and `--old-confirmation` pointing to the earlier
cold study's development/confirmation panel files. All take `--library`; readiness
and study also take `--environment` for the existing runtime environment file.
The original scientific invocation was:

```sh
MASK_ROOT=/home/ashrimp/Srv/aic-band-mask-training-20260914
env CUBLAS_WORKSPACE_CONFIG=:4096:8 "$MASK_ROOT/tools/factory-edit-pilot/.venv/bin/python" "$MASK_ROOT/tools/factory-edit-pilot/operations/cold_mask_study.py" study --output "$MASK_ROOT/artifacts/study" --library "$MASK_ROOT/target/release/libaic_pilot_native.so" --environment /home/ashrimp/Srv/aic-declarative-batch-20260913/.tracking.env --readiness "$MASK_ROOT/artifacts/readiness/readiness.json" --panels "$MASK_ROOT/artifacts/panels/panels.json"
```

Use the same invocation to resume an interrupted scientific run from its normal
checkpoints and original clock. A completed study returns without retraining.
