# Frozen-collection successful-action imitation screen

Status: closed. Successful-action imitation alone passes the predeclared
frozen-collection learner screen. Fresh on-policy generations and untouched
held-out evaluation remain required before changing production training.

## Question and method

Does the learner lose previously solved layouts because it trains on diffuse
policy targets, including successful episodes? Compare four treatments on
three independent retained 16-case search collections (`s1`–`s3`). Each
treatment starts from the same generation-87 lower and outer checkpoints,
trains the outer model for 100 epochs/1600 updates with lower frozen, and is
evaluated on the same new 64-case training-partition diagnostic panel. The
diagnostic problems exclude all 48 source problems. The four treatments are:

- `current`: original policy targets; failed-episode policy loss enabled.
- `failed-off`: original targets; failed-episode policy loss disabled.
- `success-on`: successful episodes put 80% policy-target mass on their
  recorded selected action and keep 20% of the original distribution;
  failed episodes and their policy loss remain unchanged.
- `success-off`: the successful-action target plus disabled failed-episode
  policy loss.

The transformation changes only `policy_target` values, verified against
each original report. It changes 61, 60, and 59 successful roots, while
leaving 93, 85, and 94 failed roots untouched across the three streams. The
mean total-variation changes of successful-root targets are 0.696, 0.684,
and 0.689. Search witness, value supervision, initial models, case identities,
and diagnostic panel remain paired.

The starting checkpoint completes 25/64 diagnostic cases. Each cell below
reports completed cases and, in parentheses, the number of those 25 starting
successes lost after one generation.

| Treatment | s1 | s2 | s3 | Mean completion | Mean lost start successes |
| --- | ---: | ---: | ---: | ---: | ---: |
| Current | 30 (13) | 21 (16) | 24 (13) | 25.0 | 14.0 |
| Failed-off | 25 (10) | 24 (12) | 23 (12) | 24.0 | 11.33 |
| Success-on | 29 (10) | 29 (12) | 27 (11) | 28.33 | 11.0 |
| Success-off | 24 (13) | 17 (15) | 25 (15) | 22.0 | 14.33 |

The predeclared promotion screen requires a mean of at least three fewer
lost starting successes than `current`, positive retention in at least two
streams, and mean completion no worse than `current` minus two. `Success-on`
passes exactly at the retention threshold: fewer losses by 3, 4, and 2
(mean 3), and completion differences -1, +8, and +3 (mean +3.33).
`Failed-off` misses the retention threshold (mean 2.67 fewer losses).
`Success-off` fails both retention and completion. The combination is not
additive: relative to `failed-off`, `success-off` loses three more starting
successes in each stream and averages two fewer completions.

## Interpretation and limits

The consistent retention improvement from successful-action imitation is
evidence that the original diffuse target was part of the learner problem.
It is not yet a recovered training setup. These models learned from frozen
collections produced by the same earlier policy, once. The policy must now
collect its own later generations, where the changed action distribution can
alter data quality, success frequency, and retention. The diagnostic panel
also reuses a single ID namespace and search seed; the independent namespace
study showed that absolute search counts can change substantially with root
noise. Keep paired arms and multiple seeds in the continuation.

Training loss is not directly comparable between treatments with failed
policy loss enabled and disabled, because the objective changes. Completion
and paired retention are the decision metrics.

## Reproduction and provenance

- Experiment source commit `09935a74`; runner
  `tools/factory-edit-pilot/hierarchical_success_imitation.py` and
  the retained search collections under
  `~/Srv/aic-target-causal-20260925/artifacts/recovery-search-v2/raw/`.
- Remote output `~/Srv/aic-target-causal-20260925/artifacts/recovery-success-imitation`.
- From that workspace with its documented LibTorch environment:
  `python3 tools/factory-edit-pilot/hierarchical_success_imitation.py prepare --output artifacts/recovery-success-imitation --experiment-commit 09935a74`,
  then `run` and `summarize` with the same `--output`.
- [Protocol](2026-09-25-hierarchical-success-imitation-protocol.json) holds
  exact source, binary, search, model, configuration, helper and diagnostic
  hashes. The generated [failed-off configuration](../benchmarks/hierarchical-target-causal/success-imitation-failed-off-generation.json)
  is retained as a benchmark source asset. [Progress](2026-09-25-hierarchical-success-imitation-progress.json)
  holds all 12 outcomes, per-case starting-success masks, report and checkpoint
  hashes. [Summary](2026-09-25-hierarchical-success-imitation-summary.json)
  applies the fixed gate. Per-stream target manifests:
  [s1](2026-09-25-hierarchical-success-imitation-s1-transform.json),
  [s2](2026-09-25-hierarchical-success-imitation-s2-transform.json),
  [s3](2026-09-25-hierarchical-success-imitation-s3-transform.json).
- W&B [status](2026-09-25-hierarchical-success-imitation-wandb-observer.json)
  records remote acknowledgment through step 12;
  [run](https://wandb.ai/acrylicshrimp-personal/endfield-aic-layout-gen/runs/22390be6b11cd3a48edf48c3).
