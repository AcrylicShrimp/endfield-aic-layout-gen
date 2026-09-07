# Frozen-update gradient audit

Status: declared before execution, 2026-09-07. Offline diagnostic, not training selection.

## Question and fixed inputs

On both complete-factory capsules from `layout-direct-update-audit-02`, distinguish
task-gradient magnitude, entropy-gradient magnitude, cancellation, and Adam carryover.
The seven- and 59-facility roots and all 256 recorded actions are fixed, not resampled.
Require the capsule's expected SHA-256 and the existing exact-update verifier first.
Do not edit the recorded implementation, factories, model files, or source artifacts.

## Arms and budget

Restore the identical before-model, Adam state and RNG for each arm:

- Recorded prefix credit with recorded entropy (exact recorded update control).
- Recorded prefix credit without entropy.
- Zero credit with recorded entropy (entropy-only).
- Zero credit without entropy (Adam carryover control).
- Zero credit without entropy, clearing Adam moments (true no-update control).
- Final tournament ranks on the same quality/futility outcomes, without the prefix baseline.
- Final quality-only tournament ranks, without entropy (separately changes tie handling).
- Every permutation of the four complete recorded credit sequences across trajectories,
  without entropy: 24 assignments including identity, not 24 independent datasets.

Also separate each trajectory's task contribution, using the original total action-count
normalization. Maximum four trajectories and 512 actions per capsule. No environmental
actions, new training, architecture/hyperparameter sweep, or held-out test access.

## Outputs, invariants and failures

CLI inputs: `--capsule`, `--sha256`, exclusive-create `--output`. Version-1 finite JSON:
source references, exact replay result, arm loss/gradient/parameter-displacement norms,
old-prefix conditional KL/TV by decoder stage, component cosine and decomposition residual,
per-trajectory gradient norms and pairwise cosines, and all credit-permutation results.
Gradients are loss-descent gradients, not return-ascent gradients. Undefined cosine is null.
Normalize trajectory contributions by the whole group's action count; do not compare a
summed group gradient against per-trajectory means with different denominators.

Gradient decomposition allows float32 accumulation rounding (relative residual <= 1e-5).
The combined arm must reproduce the saved gradient and model/optimizer snapshot exactly.
Fresh-moment zero/zero must leave all model parameters unchanged. Existing moments may move
parameters even under zero current gradient; that is not learned signal from this batch.
Permutation controls preserve each credit sequence and marginal coefficients, but destroy
its assignment to behavior; no permutation significance test or SNR estimate is claimed.

Fail closed on changed sources, capsule hashes, behavior, nonfinite values, failed decomposition,
or failed controls. Emit stable code `gradient-audit-failed`, severity `error`, stage
`frozen-update-gradient-audit`, message and capsule entity. Never overwrite an output.

## Decision boundary

Report both capsules regardless of result. An entropy-to-task norm ratio measures this
batch, not long-run training causality. A change in old-prefix probabilities or a nonzero
gradient does not establish layout improvement, useful delayed credit, or generalization.
These correlated trajectory contributions are not independent gradient replicates; do not
label their dispersion estimator variance or gradient SNR. Repeated independently sampled
batches at a frozen policy/root and prospective quality controls remain the next gate.
