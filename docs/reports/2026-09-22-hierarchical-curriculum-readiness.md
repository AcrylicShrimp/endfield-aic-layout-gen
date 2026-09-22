# Hierarchical curriculum readiness

Run outcome: the canonical run was stopped after 11 committed generations when
same-level training and fixed held-out completion regressed. See
[`2026-09-22-hierarchical-curriculum-early-regression.md`](2026-09-22-hierarchical-curriculum-early-regression.md).

## Question and decision

This cycle asked whether the solver-augmented hierarchical learner was ready to
leave a four-case overfit proof and begin fresh production-graph training while
learning compact completed layouts. The result is **ready for the canonical
100-generation run**, with held-out completion and layout quality treated as
measurements rather than a claim of game-scale generalization.

The run uses 32 freshly sampled cases per generation, a 128-case rolling
training-success window, a 75% promotion threshold and a fixed disjoint
128-case held-out panel. Across-generation repeats are allowed because they are
natural samples from the finite recipe distribution. Cases remain unique
within each generated panel.

## Method and retained configuration

The tracked source configurations are:

- [`curriculum.json`](../benchmarks/hierarchical-curriculum/curriculum.json):
  100 generations; levels `1/1/4/4`, `2/2/6/6`, and `3/3/8/8` for
  minimum/maximum facilities, terminals and connections; held-out evaluation
  every five generations.
- [`search.json`](../benchmarks/hierarchical-curriculum/search.json): 64 MCTS
  simulations per move, six-macro horizon, 24-macro episode limit, eight exact
  routing workers and no beam search.
- [`generation.json`](../benchmarks/hierarchical-curriculum/generation.json):
  200 alternating-update epochs, 16 root states per batch, epoch checkpoints
  every 20 epochs, lower learning rate `1e-5` and outer learning rate `3e-4`.
- [`outer-warm.json`](../benchmarks/hierarchical-curriculum/outer-warm.json):
  the exact search contract used to prepare the initial outer checkpoint.

The lower checkpoint came from the completed hierarchical route-guidance warm
start. The outer warm start trained on 18 states and evaluated four separate
states containing completion choices with different physical costs. Its
selected completing actions improved from 0/4 to 4/4 and its mean selected
utility from `0.4990` to the oracle value `151.6132`. The retained starting
checkpoint hashes are:

```text
lower  7bf16ae8e13f3b5ddd896d72ffa03fcb135ec582fdea2b8e670331d4e6de0bf7
outer  d86a6016daa029e92669306dd7205c6b15a3e6660bf967ad3bd58e03424b8b35
heldout panel 4358387a8a8fa211d480b620b2155a61b05d79e117b956e6cfc61182a934c0cc
```

## Correctness and continuity changes

The eight-head outer value model predicts completion, progress, blockage,
footprint, transport cells, turns and uncertainty. Exact incomplete and model
leaf values remain at or below `1`; independently validated completions occupy
`(1, 2]`. Completed values order footprint first, transport cells second and
turns third. Terminal MCTS revisits recompute that exact value, so repeated
visits preserve quality. Checkpoint schema v3 rejects older six-head models.

The existing native session, validator, exact route candidate generator,
batched model inference, persistent model instances, eight-worker routing pool,
compiled inference, frozen-collection lineage checks and atomic generation
checkpoints were retained. New curriculum code orchestrates those components
rather than replacing them. Normal collection requires MCTS-only search. Model
promotion remains all-or-nothing, and held-out outcomes cannot alter the
training-success window or curriculum level.

Recovery now recognizes the same dot-prefixed pending generation directory as
the generation trainer. It reconstructs a missing latest-checkpoint pointer
when a complete checkpoint exists and safely restarts when interruption occurs
before the first checkpoint. Final reports are regenerated from committed
summaries after recovery.

## Integrated smoke result

A one-generation release-mode CUDA smoke used the canonical search and update
settings, 32 fresh level-zero training cases and all 128 held-out cases. It
completed successfully:

| Measure | Training | Held out |
| --- | ---: | ---: |
| Completed | 16/32 (50.0%) | 16/128 (12.5%) |
| Mean footprint of completions | 168.5625 | 122.875 |
| Mean transport cells | 51.25 | 43.0 |
| Mean turns | 5.5625 | 5.875 |
| MCTS samples/minute | 377,938 | 308,276 |

Distillation used 183 root states, 17,356 outer actions and 340 lower routes.
Across 200 epochs, outer loss decreased from `3.69675` to `2.01611`, and lower
loss from `109.70975` to `11.44511`. Update time was 19.54 seconds. The published
generation checkpoint hashes were
`68de8cbe83c8af5a60eb4e89964034b41f29985229413730e29d0cb677814ca9`
for the lower model and
`91e18788d2d6b9ad73c3ac445a46b2787367be7fde1da563ff1e0217784477c9`
for the outer model.

The first smoke attempt exposed one bounded artifact defect: held-out evaluation
retained 1,161 training-only MCTS root records and produced a 95,203,823-byte
report, exceeding the 64 MiB curriculum input limit. Evaluation now omits root
records. The corrected held-out report was 1,832,359 bytes while preserving the
same 128 case outcomes and aggregate quality metrics. Training collection still
retains its 183 root records for distillation.

## Tracking, reproduction and verdict

The independent W&B observer exported the corrected smoke and verified remote
acknowledgement at
[run a116c8dea151d753d3d5e103](https://wandb.ai/acrylicshrimp-personal/endfield-aic-layout-gen/runs/a116c8dea151d753d3d5e103).
It reads only atomically committed summaries; observer delay or failure cannot
block training or change local recovery state.

The production command, run from the repository root with the retained LibTorch
environment, is:

```bash
target/release/aic-cli learning train-hierarchical-curriculum \
  --config docs/benchmarks/hierarchical-curriculum/curriculum.json \
  --search-config docs/benchmarks/hierarchical-curriculum/search.json \
  --generation-config docs/benchmarks/hierarchical-curriculum/generation.json \
  --sampling docs/benchmarks/direct-constructive-full-graphs/sampling.json \
  --heldout-panel artifacts/curriculum-v1-heldout-panel.json \
  --initial-lower-output artifacts/overfit-v2/warm/lower-v2 \
  --initial-outer-output artifacts/curriculum-v1-outer-warm \
  --catalog-root data/game/normalized \
  --output artifacts/hierarchical-curriculum-v1 \
  --device cuda:0
```

The initial completion rates are a baseline, not an expected final score. The
run is intended to answer whether alternating MCTS distillation raises fresh
training completion, transfers to the fixed held-out panel, advances through
the three complexity levels and improves completed-layout costs without losing
completion. One smoke generation cannot establish any of those learning-curve
claims; the committed generation series is the required evidence.
