# Frozen-batch gradient controls

Status: declared offline controls passed; learning efficacy remains unestablished.
Date: 2026-09-07. Contract: `gradient-audit-contract.md`.

## Decision

Do not blame entropy dominance or gradient clipping for these two updates. The task
gradient dominates the weighted entropy gradient on both recorded complete-factory batches,
and disabling entropy barely changes the combined update direction. However, Adam carryover
alone changes parameters on the second batch. Nonzero parameter movement, even under an
entropy-only arm, is not evidence that the current batch taught useful actions.

This is not a long-run causal ablation or a capacity diagnosis. Proceed to independent
frozen-policy rollout batches and prospective quality evaluation, not a larger model or
arbitrarily longer training on the basis of these controls.

## Fixed evidence and execution

Inputs are the original two capsules listed in `update-replay-report.md`: four 32-action
trajectories on a seven-facility factory, then four on the 59-facility target. Both exact
source/root/behavior/credit/model/Adam replay checks passed again. Every arm restores the
same before-snapshot; no arm's update becomes the next arm's starting point.

Seven named arms and all 24 assignments of complete credit sequences to the four recorded
trajectories ran per capsule. The identity assignment is an explicit control, leaving 23
nonidentity assignments. No environmental action was executed, no new root generated, and
no checkpoint, policy, credit rule, executor or factory artifact was modified. Previously
validated complete geometry and the visually inspected enabling port-change sequence are
unchanged; there is no new geometry to inspect or claim as a packing improvement.

Final JSON SHA-256 values (under `target/layout-direct-update-audit-02/`):

- `gradient-audit-final-0001.json`: `1639339f61f092bafee18e49c64bca4c7ed7416ed2a71dfb39cd39b89610e7d1`.
- `gradient-audit-final-0002.json`: `14af85d99a7db547e3515088630324730cd742403a49d36f5056515e966fdea0`.

These runs took 34.84 and 104.43 seconds respectively, with overlapping local work; they are
not isolated throughput benchmarks. The original capsules and source references still match.

| Measurement | Seven facilities | 59 facilities |
| --- | ---: | ---: |
| Task gradient L2 norm | 0.03227177 | 0.04565965 |
| Weighted entropy gradient L2 norm | 3.74968e-8 | 1.46234e-8 |
| Entropy/task norm ratio | 1.16191e-6 | 3.20270e-7 |
| Combined vs task-only parameter-delta cosine | 0.99999719 | 0.99998551 |
| Zero-current-signal parameter-delta L2 norm | 0 | 0.30653881 |
| Zero-current-signal, fresh-moments parameter delta | 0 | 0 |
| Recorded combined parameter-delta L2 norm | 0.45907658 | 0.35214939 |
| Task-only mean kind-distribution TV on old prefixes | 0.00117120 | 0.00162625 |

Norms summarize full parameter vectors; they are not normalized policy-quality units.
Raw diagnostic norms reduce the float32 gradient elements in float64. The trainer's
float32 norm reduction is also retained verbatim in each arm's `update` object.
Combined gradients decompose into task plus weighted entropy with relative residuals
3.32e-7 and 6.46e-7, inside the preregistered 1e-5 float32 accumulation tolerance.
The recorded combined arm reproduces the exact saved gradient fingerprint and after-snapshot.

The second batch's entropy-only parameter delta is 0.30653916, almost the same norm as
the zero/zero carryover arm. Their vector distance is 0.00047570. Do not call the whole
entropy-only displacement an entropy effect. Similarly, ratios of displacement norms do
not partition causal contributions: Adam is nonlinear and directions differ.

## Credit and trajectory controls

Removing the prefix baseline while preserving the same final quality/futility outcomes
changes the task-gradient direction: cosine to the recorded prefix gradient is 0.77739
and 0.60178. The corresponding norms are 0.07212518 and 0.06796344. Neither a larger norm
nor a changed direction establishes lower estimator variance or better layout learning.

The quality-only terminal arm equals the same-outcome terminal arm on the small batch.
On the target, workers 2 and 3 tie on final quality, but futility orders them. Removing
that tie-break changes terminal ranks from `[1/3, 1, -1/3, -1]` to
`[1/3, 1, -2/3, -2/3]`. This is a different objective, not just removal of the baseline.

The norms of the summed task contributions are 0.63674 and 0.62909 times the sum of
their individual norms. Some pairwise trajectory cosines are negative. These are
correlated contributions sharing tournament outcomes, not independent gradient samples;
their cancellation must not be labeled gradient SNR or estimator variance.

Credit reassignment preserves all coefficient sequences but can strongly change direction.
Across nonidentity assignments, cosine to the original task gradient ranges from -0.95182
to 0.94830 on the small factory and -0.82454 to 0.84837 on the target; respectively 13/23
and 10/23 are negative. All shuffled controls still produce nonzero gradients. This
demonstrates sensitivity to assignment, not that the original assignment is useful or
statistically superior. No permutation p-value, population uncertainty, or held-out benefit
is inferred from these fixed recorded actions.

## Verification and limitations

- Nineteen Python tests pass, including independent joint-loss differentiation on actual
  complete-root observations, behavior-corruption rejection, task/entropy decomposition,
  empty versus nonempty Adam-moment zero-signal controls and undefined-cosine handling.
- Fifteen Node tests and `cargo check --workspace` / `cargo test --workspace` pass;
  two existing Rust tests remain ignored. No Rust source edits or new dependencies.
- CLI runs check expected capsule hashes, preserve exclusive output creation, and report
  all arms. The diagnostic leaves the recorded main implementation untouched.
- An initial test exposed cosine roundoff just outside [-1,1]; reduction now uses float64
  and clamps only that numerical range. Final artifacts repeat the same predetermined
  inputs and arms after this diagnostic fix; no outcome-based run selection occurred.
- The 2026-09-02 constructive-planner design and AGENTS policy were checked: this offline
  learner diagnostic changes no growth semantics, routing order, objective priority,
  production boundary or local/global infeasibility claims. Existing user edits are preserved.

## Reproduction

Run each command in a separate process and use a fresh output path if repeating:

```sh
uv run --frozen --project docs/benchmarks/layout-learning python docs/benchmarks/layout-direct/gradient_audit.py \
  --capsule target/layout-direct-update-audit-02/update-0001.json \
  --sha256 444455d32a91864944278feabba393081f891634f13aa0fa32cfe8d44cef0546 \
  --output target/layout-direct-update-audit-02/gradient-audit-final-0001.json
uv run --frozen --project docs/benchmarks/layout-learning python docs/benchmarks/layout-direct/gradient_audit.py \
  --capsule target/layout-direct-update-audit-02/update-0002.json \
  --sha256 b746b3e6303ae14399cef77a234436c9a5b59aceb249bc8b588b12eb0b2b4789 \
  --output target/layout-direct-update-audit-02/gradient-audit-final-0002.json
uv run --frozen --project docs/benchmarks/layout-learning python -m unittest discover \
  -s docs/benchmarks/layout-direct -p 'test_*.py'
```

## Next gate

Preregister independent rollout-batch seeds at a frozen policy and unchanged complete roots,
with the target included explicitly rather than disguised as fresh generalization. Compare
the same-outcome prefix and terminal estimators on each batch, plus zero/shuffled controls,
with entropy disabled for task-signal causality. Keep model initialization, actions, root
selection and prospective evaluation RNG separate. Use batch-level replicates, not steps,
for direction agreement and uncertainty. Evaluate candidate updates on separate matched
rollouts, retaining and validating complete factories; directly inspect any claimed geometric
win. Do not penalize the previously verified useful temporary port detour merely because its
immediate score is worse. Larger stream admission remains a separate unapproved policy change.
