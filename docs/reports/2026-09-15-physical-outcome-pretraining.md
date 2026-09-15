# Physical outcome pretraining result

## Result

The registered two-seed experiment completed, but the transfer gate failed. The
multi-head model learned a useful completion ranking and beat both completion
baselines. Its continuous area, transport and turn predictions were worse than
the constant baseline, so the encoder must not be transferred into PPO under the
accepted gate.

The run used dataset manifest SHA-256
`11a49bcc051349e81b6a96be430b7ed7f1c3b6a76152a8f308c2a9fd9d1c4b3c` and
registered config SHA-256
`149c4b9dad6426d63f0445b3201f53329c48e517589ce83b3ee1aef8ed19f3b9`.
It ran on one NVIDIA GeForce RTX 5090 with LibTorch 2.13 and `cuda:0`.

## Fixed experiment

- Seeds: `930000`, `930001`
- Schedule: 12 epochs, four roots per batch
- Split: 64 training, 16 development and 16 confirmation roots
- Model: the existing width-40, two-round graph encoder plus 12 disposable
  affine outcome heads
- Baselines: training-label constant estimates and a 42-feature affine
  geometric predictor
- Selection: masked weighted development loss, with confirmation decoded only
  after model and baseline hashes were locked
- Limits: 30 minutes, 8 GiB RSS and 512 MiB retained output

The command completed in 15.61 seconds. Peak process-tree RSS was 2,044,522,496
bytes and retained output was 1,141,679 bytes.

## Confirmation results

The completion set contained 499 labels over all 16 roots: 28 positive and 471
negative. Continuous metrics had 28 labels over nine roots.

| Predictor | Completion log loss | AUROC | Mean normalized metric MAE |
| --- | ---: | ---: | ---: |
| Multi-head, two-seed mean | 0.190426 | 0.860409 | 0.041902 |
| Geometric, two-seed mean | 0.246165 | 0.551520 | 0.242024 |
| Constant | 0.214355 | 0.500000 | 0.023405 |

The multi-head completion log loss improved by 11.2% over the constant and by
22.6% over the geometric baseline. Mean Brier score was 0.050448 and mean
ten-bin ECE was 0.004761.

| Seed | Best model epoch | Completion log loss | Brier | ECE10 | AUROC |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 930000 | 12 | 0.190697 | 0.050498 | 0.006958 | 0.875931 |
| 930001 | 12 | 0.190155 | 0.050398 | 0.002563 | 0.844886 |

| Metric | Seed 930000 normalized / raw MAE | Seed 930001 normalized / raw MAE | Constant normalized / raw MAE |
| --- | ---: | ---: | ---: |
| Area | 0.035356 / 56.120 | 0.056786 / 89.766 | 0.033965 / 54.622 |
| Transport cells | 0.040866 / 26.201 | 0.032129 / 18.124 | 0.020464 / 11.469 |
| Turns | 0.039848 / 2.148 | 0.046427 / 2.578 | 0.015786 / 0.632 |

Size-8 and size-16 aggregate strata were both two-class. Family 0 and the
pre-action pending stratum contained no completion positives, so their AUROC is
recorded as `null`; the report makes no accuracy claim for these or other
single-class intersections. Family 1, family 2 and the pre-action complete
stratum retained their two-class AUROC values. The ignored artifact contains the
full size, family, state and combined breakdown with every head's label and root
denominator.

## Transfer gate

| Clause | Threshold | Observed | Pass |
| --- | --- | ---: | --- |
| Completion beats constant | `< 0.214355` | 0.190426 | Yes |
| Completion beats geometric | `< 0.246165` | 0.190426 | Yes |
| AUROC | `>= 0.60` | 0.860409 | Yes |
| Mean metric MAE improves on constant by 5% | `<= 0.022235` | 0.041902 | **No** |

The final gate verdict is **failed**. The experiment stops here; no encoder-only
PPO transfer experiment is authorized by this result. The completion signal is
promising, but only 28 continuous labels were available and neither seed beat
the constant baseline across the three normalized metric errors.

## Reuse and verification

The implementation retained the established packed graph builder, shared graph
encoder, optimized masked-reference path, terminal-pair encoder, foreach Adam,
resource guard, byte ledger and safetensors format. Dataset admission now exposes
one verified index and bounded root reader; the existing full validator delegates
to the same hash, framing and binding implementation.

Two focused LibTorch tests passed: split isolation with native label masks and
denominators, and terminal-port binding with shared-family gradients and the
encoder-only allowlist. The registered release command supplied the integrated
CUDA, dataset, checkpoint, resource and publication check. Repository workspace
checks and tracked-file policy checks also passed after the final source change.

The retained local result is under
`artifacts/physical-outcome-pretraining-v1`. Its report SHA-256 is
`69d9fc02c1268ce002e7d21f714b37bca3fa8a975dda12f75e3179956d816131`.
The locked encoder SHA-256 values are
`43311bb2af400d4d201dfcbe925022cb7c8b5eb6ae0375f16836e091a3f5faa9`
and `d7543bd4571123045583921e7c9062fdacac62fad5177ba62cee05213bfdaea9`.
