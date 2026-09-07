# Broader continued-improvement RL pilot

This is an isolated, bounded experiment using the existing layout RL learner and local actions.
Read the [contract](contract.md), [experiment design](experiment-plan.md) and
[measured pilot gate report](gate-report.md).
It does not change production planning, geometry semantics, reward priority or game catalogs.

Run from the repository root with the existing locked CPU environment and release validator:

```sh
cargo build --release -p aic-cli --bin aic-witness-probe
node docs/benchmarks/layout-rl-next/corpus.mjs \
  --plan docs/benchmarks/layout-rl-next/corpus.json \
  --output-directory target/layout-rl-next-corpus-01

uv run --frozen --project docs/benchmarks/layout-learning python \
  docs/benchmarks/layout-rl-next/pilot.py \
  --plan docs/benchmarks/layout-rl-next/pilot.json \
  --output-directory target/layout-rl-next-pilot-01
```

All output directories/files must be new. These paths identify the measured run and fail
if reused. To repeat, change explicit output paths and the corresponding plan references.
The previous corpus and its source artifacts must still exist and match their hashes.
No missing generator output is silently substituted. See the prior [setup](../layout-rl/README.md).

The corpus builder defines new catalog-derived production requests before constructing them.
Requested procurement depth produces explicit intermediate imports, not dropped obligations.
Every completed layout is checked by Rust and the original-request coverage checker. Cases
outside the unchanged constructor domain or budget remain in `report.json`; they do not
become training examples or global-infeasibility claims. Old split assignments are unchanged.

The pilot launches H16/H32/H64 terminal-credit runs and an H16 zero-signal control, with
8,192 online actions per arm. It uses fresh weights to isolate the new experiment; historical
models and their artifacts are preserved. It then evaluates final checkpoints and nonlearning
controls on **validation only**, at 128/512/1,024 proposals. No best-seed/checkpoint selection.
The existing trainer samples roots uniformly; the small pilot's different group counts expose
different numbers of distinct roots. This is a feasibility pilot, not a causal horizon result.
Full horizon controls need matched per-root action exposure before test evaluation.

`audit.json` independently checks every saved training/evaluation best incumbent, source hashes,
exact score, immutable semantics, split usage and training action totals. It reports late
improvements and no-improvement tails; neither certifies local or global optimality. `report.json`
records successful process completion, not learned superiority. Worker and orchestration guards
are explicit; if a guard fails, completed artifacts remain available and no full run starts.

## Verification

```sh
node --test docs/benchmarks/layout-rl-next/*.test.mjs
uv run --frozen --project docs/benchmarks/layout-learning python \
  -m unittest discover -s docs/benchmarks/layout-rl-next -p 'test_*.py'
node --test docs/benchmarks/layout-rl/*.test.mjs
cargo check --workspace
cargo test --workspace
```

Full model/trajectory artifacts live under `target/`, outside Git. The committed gate report
and external [results snapshot](../../../artifacts/benchmarks/layout-rl-next/pilot-results.json) retain measurements and hashes; they cannot replace
those inputs for exact replay.

To check the reported fields of that specific historical pilot, supply its external
snapshot explicitly. This audit is not part of the unit suite, does not inspect actual
artifact bytes, and is not a quality expectation for other runs:

```sh
node docs/benchmarks/layout-rl-next/audit-pilot-results.mjs \
  --input artifacts/benchmarks/layout-rl-next/pilot-results.json
```
