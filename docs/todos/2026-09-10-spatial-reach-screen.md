# Actor spatial receptive-field screen

Status: in progress.

## Context and contract

The user approved implementing and running the four treatments in
`docs/designs/2026-09-10.16-spatial-receptive-field-review.md`.
Inputs and outputs retain the native observation, complete action domain, mask,
current/history visibility, reward and validated region executor. The explicit
`ppo-actor-spatial-reach-v1` model specification declares CNN layers/dilations
separately from graph layers. Protocol source/spec hashes bind checkpoints;
strict state loading rejects incompatible parameter shapes/keys.

All actors retain the original two-convolution stem with its one GroupNorm.
Each additional layer computes `x + SiLU(Conv3x3_dilation(x))`, with no added
normalization or downsampling. Thus both four-layer treatments have exactly the
same residual structure and parameter count. The eight-layer treatment has six
additional blocks. The critic remains the original two-convolution 2/32 model.
Its initialization uses `seed + 100000` in a separate RNG context; record actual
initial hashes. This is a fresh baseline as well as three fresh treatments.

Run each with 32 batches and evaluations at 0/8/16/24/32, using all 1,056 frozen
starts from the region pilot. Each uses seed 17, width 32, graph depth two, eight
workers, CUDA FP32 and the unchanged PPO settings. Run variants serially to avoid
GPU contention; record order. These factories form a selection set and one
initialization seed does not establish generalization or asymptotic capacity.

Failures use the harness's structured stage/code/severity/entity diagnostics.
Invalid model specifications fail before preparation. Numerical, parity,
continuation or resource failures block the comparison until resolved without
changing the action or executor contract.

## Completion criteria

- Shared raster execution in single-state collection and batched learning.
- Finite forward/gradients, probability normalization, cross-world isolation,
  remapped-node permutation and chunk agreement for all variants.
- Profile retained median/p95/largest worlds on CPU and CUDA before training.
- Separate two-batch native CUDA readiness and exact Adam/RNG continuation for
  all four variants; actual input sizes exercise large candidate sets.
- Four complete fresh 32-batch runs, checked initialization and frozen inputs.
- Paired best-area and learning-delta comparisons, action and facility-size
  breakdowns, rejection categories, timing, memory and KL behavior.
- Retain checkpoints and compact provenance; write findings and commit.

## Activity

- 2026-09-10: Approved experiment; implemented shared spatial forward and explicit
  model specifications. All 53 PPO Python tests and workspace check/tests passed
  (548 Rust tests, five ignored). Server profiling and CUDA readiness are next.
