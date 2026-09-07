# Online multi-step layout RL

This isolated CLI experiment starts from **complete factories from both existing generators**.
It may explore worse valid private states, but only returns a validated best incumbent.
Production commands and the global exact research solver do not call this learner.

Read [findings](findings.md), the [contract](contract.md), the frozen
[experiment matrix](experiment-plan.md) and [learning rationale](research.md).

## Setup and roots

Run from the repository root. Reuse the previous experiment's locked CPU PyTorch environment:

```sh
uv sync --frozen --project docs/benchmarks/layout-learning
cargo build --release -p aic-cli --bin aic-witness-probe
node docs/benchmarks/layout-rl/import.mjs \
  --plan docs/benchmarks/layout-rl/plan.json \
  --output-directory target/layout-rl-roots-03
```

Every output directory/file must be new. The displayed paths identify the measured run and
will intentionally fail if reused. Change output paths and corresponding explicit config
references for a new experiment. `plan.json` names existing generator reports and original
requests; it does not fabricate substitute fixtures when an upstream artifact is missing.
The chain source is `target/chain-m5-comparison-release/report.json`, produced by the
[chain prototype comparison](../chain-prototype/README.md). The older sources are
`constructive-factory-closure/matrix.json` and `output-space-refined.json`, which name their
selected witness directories under `target/`. Restore/regenerate those artifacts or name
other complete outputs in a new plan. Missing inputs are reported with their paths.

The importer reads current external catalogs, independently checks all selected geometry and
original-request coverage, preserves real external obligations and recomputes one common score.
Whole-witness rotation/translation equivalents are deduplicated before assigning splits.
Generator reports are provenance, never a replacement for fresh validation.

`roots.json` contains the original requests/witnesses, split identities, scores and input hashes.
Changing any pinned source, catalog or validator requires a fresh import. Runtime implementation
hashes are additionally recorded and checked by every environment worker.

## Online training and fixed-checkpoint evaluation

```sh
uv run --frozen --project docs/benchmarks/layout-learning python \
  docs/benchmarks/layout-rl/train.py \
  --roots target/layout-rl-roots-03/roots.json \
  --config docs/benchmarks/layout-rl/training.json \
  --output-directory target/layout-rl-training-main-02

uv run --frozen --project docs/benchmarks/layout-learning python \
  docs/benchmarks/layout-rl/evaluate.py \
  --roots target/layout-rl-roots-03/roots.json \
  --config docs/benchmarks/layout-rl/evaluation.json \
  --output-directory target/layout-rl-evaluation-main-02
```

Each sampled action actually runs the local repair kernel and, when it produces new geometry,
the independent persistent Rust validator. There is no cached candidate-outcome dataset.
All model weights are JSON, with exact reload checks; no pickle loading is used. Optional
NumPy absence produces a PyTorch warning but is not a failure for this implementation.

Training emits eight final checkpoints, per-group terminal signals/loss/work, validation
curves, and one immutable current/best witness plus action trace per trajectory. Evaluation
emits all 792 main rows: model seed, sampling seed, budget, exact score, incumbent history,
charged work, termination reason and fresh full-factory audit. No test-based checkpoint selection.

For the same-case delayed-credit experiment, run `detour-training.json` into
`target/layout-rl-training-detour-03` and `detour-evaluation.json` into
`target/layout-rl-evaluation-detour-03`. Its two-action horizon is intentionally different
from the main horizon 16. Establish the conditional local proof separately:

```sh
node docs/benchmarks/layout-rl/detour.mjs \
  --roots target/layout-rl-roots-03/roots.json --root chain-citron-51 \
  --output-directory target/layout-rl-detour-04
```

## Independent replay audit

```sh
node docs/benchmarks/layout-rl/audit.mjs \
  --roots target/layout-rl-roots-03/roots.json \
  --evaluation target/layout-rl-evaluation-main-02/report.json \
  --output-directory target/layout-rl-audit-main-01

uv run --frozen --project docs/benchmarks/layout-learning python \
  docs/benchmarks/layout-rl/audit_policy.py \
  --audit target/layout-rl-audit-main-01/report.json \
  --report-output target/layout-rl-audit-main-01/policy-report.json
```

Repeat for detour evaluation into `target/layout-rl-audit-detour-02`. The first audit checks
**every** final witness afresh, exact score, original graph, immutable semantics and work ledger.
It also reconstructs every method/root trajectory at the largest work cap and first sampling
seed, including best-first's branching parent-state history. Every selected transition must
match its original status, geometry and deterministic work. The second audit restores final
checkpoints and replays sampled or argmax choices from those freshly reconstructed observations.
Timed trajectories are not replayed as if deadlines were deterministic; their final witnesses
and ledgers are still all checked.

## CPU experiments and report

```sh
uv run --frozen --project docs/benchmarks/layout-learning python \
  docs/benchmarks/layout-rl/rollout.py --roots target/layout-rl-roots-03/roots.json \
  --output-directory target/layout-rl-workers-v2-train-8 \
  --workers 8 --steps 64 --repeats 4 --split train

uv run --frozen --project docs/benchmarks/layout-learning python \
  docs/benchmarks/layout-rl/cost.py --roots target/layout-rl-roots-03/roots.json \
  --output-directory target/layout-rl-cost-final-t4 --threads 4 --repeats 50

node docs/benchmarks/layout-rl/summarize.mjs \
  --plan docs/benchmarks/layout-rl/results-plan.json \
  --report-output target/layout-rl-results.json
```

Repeat workers and threads at 1/2/4/8. The large-worker sweep uses `--steps 32 --repeats 8
--split out-of-family` and output suffix `large-N`. Do not overlap the timing sweeps with
training or other substantial work. Worker comparison checks identical action/status/score/
deterministic-work traces. Tensor cost uses actual factory observations, not random tensors,
but its entropy-gradient update is a cost probe, not a quality evaluation.

[results.json](results.json) is the committed compact snapshot of all final rows, seed/control
training metrics, audits, CPU measurements and source/artifact hashes. Full trajectory/model
artifacts remain under the paths in `results-plan.json`, outside Git. Keep those directories
to rerun exact artifact audits; the compact snapshot is not a replacement for model weights.

## Stored-policy before/after diagrams

```sh
node docs/benchmarks/layout-rl/visualize.mjs \
  --plan docs/benchmarks/layout-rl/visual-comparison.json \
  --output-directory target/layout-rl-visual-comparison-01
```

This reads existing evaluation episodes; it neither retrains nor runs a new policy search.
The explicit plan selects the best observed terminal-policy result among nine model/rollout
seed combinations at 128 proposals for each of two held-out factories. These are illustrative
best cases, not typical-performance estimates or nine-run portfolio timings.
The original constructor witness is compared with the episode's **best**, not last, witness.
Both sides are checked afresh by the independent validator and original-request coverage checks;
source hashes, semantic identity and scores must match the frozen evaluation.

The command writes an inline `optimization-before-after.html` fragment and `report.json` with
four validation audits and input provenance. Each pair uses the same tile scale, separately
normalized origins, unchanged orientation, full geometry and a dashed original-footprint frame.
Select a factory or facility to inspect corresponding positions. Compact facility aliases are
display labels only; source identities, recipes, rates and ports remain unchanged.
The nine-facility example reduces area from 476 to 336 while increasing transport tiles from
36 to 65 and turns from 6 to 21. The 59-facility example improves all three score components:
`[12900, 2401, 268]` to `[9548, 1769, 236]`. Area retains first priority.

## Tests and diagnostic interface

```sh
node --test docs/benchmarks/layout-rl/*.test.mjs
uv run --frozen --project docs/benchmarks/layout-learning python \
  -m unittest discover -s docs/benchmarks/layout-rl -p 'test_*.py'
cargo check --workspace
cargo test --workspace
```

`server.mjs --roots R --output-directory D` is a single-client JSONL environment. Every request
has `schema_version: 1`, integer `id`, and `op`: `hello`, `reset`, `step`, `search`, `summary`,
`save`, `audit` or `close`. Replies echo the ID and contain either `result` or `error`.
`reset` names a root and `{steps, elapsed_ms}` budget; `step` names the unmasked action index
from the preceding observation. `search` names `greedy` or `best-first` and bounded beam/branch
sizes. Results and failures retain stable diagnostic fields: code, severity, stage, message,
entity. See [probe-contract.md](probe-contract.md) for the separate Rust JSONL protocol.

The local action set fixes ports/rotations and limits routing to the current-extent neighborhood.
It does not support temporary footprint expansion; the demonstrated detour temporarily adds
transport cost inside the current extent.
Action/token caps reject unsupported cases rather than truncate them. A resource failure or
exhausted neighborhood is never reported as whole-factory infeasibility. The exported incumbent
obeys the accepted constructive-planner improvement contract throughout private exploration.
