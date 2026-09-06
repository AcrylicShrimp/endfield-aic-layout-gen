# Isolated CPU layout-learning experiment

Start with [findings and verdict](findings.md), [community data sources](data-sources.md),
[stage contract](contract.md) and [research rationale](research.md).

This is a CLI-only research prototype, not a production optimizer. It trains small PyTorch
policies on automatically generated, independently checked internal chain relocations.
External margins remain pending; no output is a complete factory. Node handles geometry,
A* and Rust validation; Python handles model fitting, controls and inference measurements.
No game values are embedded in the new application code.

## Setup

Run from the repository root. Python dependencies are isolated and locked with uv; the virtual
Cargo workspace and production dependency graph do not change.

```sh
uv sync --frozen --project docs/benchmarks/layout-learning
cargo build --release -p aic-cli --bin aic-witness-probe
```

PyTorch may warn that optional NumPy support is unavailable. This implementation does not
use NumPy; the warning is not an environment or training failure. All measurements here use
`--device cpu`; explicitly requesting unavailable CUDA is an error, not a fallback.

## Generate data

Every output directory must be new. Replace the example output paths if they already exist.
`smoke.json` requests 36 states; `experiment.json` requests 352; `scale.json` requests 2,112.

```sh
node docs/benchmarks/layout-learning/generate.mjs \
  --plan docs/benchmarks/layout-learning/experiment.json \
  --recipes data/game/normalized/recipes.json \
  --facilities data/game/normalized/facilities.json \
  --items data/game/normalized/items.json \
  --transports data/game/normalized/transports.json \
  --components data/game/normalized/logistics-components.json \
  --validator target/release/aic-witness-probe \
  --output-directory target/learning-data
```

Artifacts include `dataset.json`, `generation.json`, `plan.json`, `provenance.json` and an
immutable evidence directory per independent probe. Probe DTOs translate the entire layout
to nonnegative coordinates; `local-witness.json` preserves the original proposal frame.
Translation never crops or changes a candidate's footprint. Dataset outcomes preserve body,
endpoint, route and work-limit failures. Failed generation writes a structured partial report.

The dataset records 12 counterfactual candidate outcomes per accepted task. Conditioning on
at least two distinct valid scores is explicit; discarded attempts remain in the work report.
Source hashes must match before training or audit, so changing environment/catalog files
requires regenerating data rather than silently using stale labels.

## Train and audit

```sh
uv run --frozen --project docs/benchmarks/layout-learning python \
  docs/benchmarks/layout-learning/train.py \
  --dataset-directory target/learning-data \
  --config docs/benchmarks/layout-learning/training.json \
  --output-directory target/learning-models --device cpu

node docs/benchmarks/layout-learning/audit.mjs \
  --training-directory target/learning-models \
  --output-directory target/learning-audit

uv run --frozen --project docs/benchmarks/layout-learning python \
  docs/benchmarks/layout-learning/verify_models.py \
  --training-directory target/learning-models \
  --report-output target/learning-audit/models.json
```

Training emits all 15 fixed seed/control runs, JSON-only model weights and normalization,
curves, full per-request metrics, selected held-out actions and CPU timing. The audit freshly
routes every unique selected action, checks the original catalogs/ledger and keeps the current
fragment when a candidate is invalid or non-improving. Model verification reloads all saved
weights and reproduces every recorded held-out selection. No pickle loading is required.

For the data-scale comparison, generate `scale.json` once, then run `training-scale-small.json`
and `training-scale-large.json` into separate fresh model directories against that same data.
Audit both. Their training limits differ (192/1,536), but held-out states, update count, seeds,
architecture and optimizer settings do not. Only the selected training subset normalizes inputs.

## Cost-only and source inspection

```sh
uv run --frozen --project docs/benchmarks/layout-learning python \
  docs/benchmarks/layout-learning/cost.py \
  --config docs/benchmarks/layout-learning/cost.json \
  --report-output target/learning-tensor-cost.json

node docs/benchmarks/layout-learning/sources.mjs \
  --manifest docs/benchmarks/layout-learning/sources.json \
  --output-directory target/learning-source-inspection
```

The tensor test measures larger random inputs only; it is not a factory-quality experiment.
Source inspection is capped at eight repository examples, one Talos list/detail record and
twelve EndfieldTools details. It performs public reads, never logs in, changes remote state,
trains on those files or implies permission to redistribute a corpus. Live counts can change;
the IndustrialPlanner source is pinned to an inspected commit.

## Tests and measured artifacts

```sh
uv run --frozen --project docs/benchmarks/layout-learning python \
  -m unittest discover -s docs/benchmarks/layout-learning -p test_model.py
node --test docs/benchmarks/layout-learning/*.test.mjs
cargo check --workspace
cargo test --workspace
```

[results.json](results.json) is a compact measured snapshot: all seed/control metrics and
checkpoint hashes, aggregate mean/min/max, generation costs, fresh audit counts and tensor
costs. Run `curves` are `[update, training mean reward, validation mean reward]`; aggregate
seed ranges are not confidence intervals. [source-results.json](source-results.json) stores
source/schema observations and response hashes, not a redistributed blueprint corpus.

Full immutable artifacts on the experiment machine:

- `target/layout-learning-data-01/` and `target/layout-learning-data-scale-01/`;
- `target/layout-learning-training-02/`, `target/layout-learning-scale-small-01/`,
  `target/layout-learning-scale-large-01/`;
- `target/layout-learning-audit-01/`, `target/layout-learning-audit-scale-small-01/`,
  `target/layout-learning-audit-scale-large-01/`;
- `target/layout-learning-cost-02.json` and `target/layout-learning-sources-01/`.

Those artifacts, the isolated `.venv` and Python caches are not committed. The faulty first
development smoke and the pre-screened-baseline training report are not authoritative results.
Authoritative reports are listed above; all measured models use fixed final checkpoints.
