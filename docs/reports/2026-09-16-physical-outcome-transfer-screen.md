# Physical outcome encoder transfer screen

## Result

The first short factory-input screen did not show a positive overall transfer at
its fixed 32-update endpoint. The pretrained arm led by 2.44 percentage points at
update 16, then finished 0.75 points below the paired cold arm at update 32.
Large factories retained a 3.75-point pretrained advantage at the endpoint while
small factories were 5.25 points worse. This one-seed result is mixed and does
not justify a longer PPO run.

The screen used the completed online encoder with SHA-256
`bc12b95499723ba27f31f381bd747d47eb21747ec7e64a1114effd39f2a4a9b2`.
The held-out panel identity was
`d3b33ee8251b90e9a6574c122f4ec82fedb40530b0ee0333d84167262ec8e637`.

## Fixed comparison

- Arms: cold initialization and the same cold actor/critic with the pretrained
  shared encoder.
- Seed: `961000` for model, action and PPO RNG ownership in both arms.
- Training: 32 streams, 16 workers, 32 actions per chunk, 32 updates and 32,768
  fresh factory transitions per arm.
- Evaluation: 16 immutable actual factory inputs, balanced between small and
  large strata, with two common action-seed replicates per input.
- Checkpoints: update 16 and update 32; neither evaluation selects or changes
  the learner.
- Configurations:
  [`physical-outcome-transfer-cold.json`](../../tools/native-training/physical-outcome-transfer-cold.json)
  and
  [`physical-outcome-transfer-pretrained.json`](../../tools/native-training/physical-outcome-transfer-pretrained.json).

| Update | Cold mean area reduction | Pretrained | Delta |
| ---: | ---: | ---: | ---: |
| 16 | 19.4093% | 21.8462% | +2.4369 pp |
| 32 | 24.3773% | 23.6236% | -0.7537 pp |

At update 16 the 32 paired evaluations produced 14 pretrained wins, 11 ties and
seven losses. At update 32 they produced seven wins, 15 ties and ten losses. The
endpoint median paired difference was zero; individual differences ranged from
-30.95 to +22.39 points, so the aggregate is small relative to case variability.

| Endpoint stratum | Cold | Pretrained | Delta |
| --- | ---: | ---: | ---: |
| Small, 16 evaluations | 42.9120% | 37.6596% | -5.2524 pp |
| Large, 16 evaluations | 5.8425% | 9.5876% | +3.7450 pp |

Both arms completed every held-out evaluation within its declared budget. The
cold arm took 85.20 seconds and the pretrained arm 90.76 seconds, including
factory generation, training, evaluation and checkpoint publication. Their peak
RSS values were 6.92 GB and 8.00 GB. Runtime and memory are observations from
single executions, not an encoder cost claim; policy-dependent trajectories
created different replay contents.

## Transfer boundary

The training configuration now accepts an optional encoder archive and required
digest. A fresh run first creates its normal cold model, Adam states and RNGs,
then replaces exactly the shared `nodes`, `edges`, `paths`, `messages`, `updates`,
`context` and `port_pair` tensors. It rejects a digest mismatch, a missing or
extra parameter, a shape mismatch or a non-finite tensor. Actor, port-actor,
critic, Adam, replay and RNG state are never imported.

This preserves the existing production admission, native edit, PPO, success
replay, curriculum, held-out evaluation and checkpoint paths. The focused
LibTorch test verified that the encoder changes while every cold-only family is
unchanged. The real CUDA runs then exercised the full factory workflow and bound
the pretrained digest in the report and checkpoint identity.

## Interpretation

The early lead and the endpoint large-factory result are consistent with useful
geometric features, but the fixed endpoint does not establish a net downstream
benefit. A second paired seed is the smallest useful confirmation before changing
the architecture or learning rate. Longer training with this exact setting is
not warranted by this screen.

Compact per-case values, report hashes and group summaries are retained in
[`2026-09-16-physical-outcome-transfer-screen-evidence.json`](2026-09-16-physical-outcome-transfer-screen-evidence.json).
