# Cumulative budget study: execution ledger

Status: stopped by the user before 1024; no completed final budget curve.
Date: 2026-09-07. Allocation and interpretation: `budget-curve-contract.md`.

## Latest decision: stopped, not awaiting completion

The user stopped training to investigate the 64-to-256 plateau, then paused diagnosis
for a committed handoff. Seeds 1901/2901/3901 ended after 399/391/389 completed updates;
all returned exit 130 following requested SIGINT. Their last full snapshots and formal
evaluations are at 256. No files were deleted and no automatic restart is authorized.
The preregistered contract remains byte-identical for evidence verification, but its
continuation instruction has been superseded by this explicit user decision.
Read [the handoff](../../handoffs/2026-09-07-layout-learning/README.md) for hashes,
results, remaining changes and the next diagnostic questions. The narrative below is
the historical execution ledger, not a claim that those processes are still alive.

## Why this experiment

The user correctly challenged the absence of a training-length comparison. Sixteen independent
single-update trials are not sixteen cumulative updates. The historical 64-update runs changed
credit, horizon and learning rate together. Neither result proves sufficient training or that
continuing training is futile. This study freezes those other choices and observes cumulative
checkpoints without efficacy-based early stopping. The unexecuted choice-fit draft is deferred,
not a prerequisite that a one-step model must pass before it may train longer.

## Frozen population

The preparation CLI completed 104 generation attempts: 64 flow rejections, 20 construction
stops, 19 accepted complete factories and one duplicate. Seven accepted single-facility factories
are learning-ineligible; the twelve eligible validation roots contain
`2,4,6,2,2,2,7,4,5,5,4,14` facilities and nine distinct target item families. The additional root
is the original complete 59-facility target. All thirteen pass fresh geometry and original
full-factory coverage audits. No sealed test was accessed or failed request cropped.

The new validation set is still entirely small-factory biased except for the explicit target.
The connected-construction admission limitation is not fixed by this experiment and prevents
a broad medium/large generalization claim regardless of the eventual curve.

Frozen artifact: `target/layout-direct-budget-evaluation-02/evaluation-set.json`, SHA-256
`a4d336b102be55f3deb7ebdacd7e10e90bcb4194076c2c74a9e6f5bba1e0acc2`.

## Startup correction, not outcome selection

The first train command (`layout-direct-budget-seed-1901-01`) exited before any training or
evaluation action: the pool constructor waited only for worker zero, but the new provenance
collector immediately read every worker's startup files. One later worker had not written its
`implementation.json` yet. Add an explicit `hello` handshake per worker before reading those
files, and reject metadata disagreement. A regression test models four independently starting
workers and fails if a file is read before its handshake.

The original output is preserved. Preparation was rerun into `layout-direct-budget-evaluation-02`
to pin the corrected source, rather than silently replacing its provenance. All thirteen full
root DTOs and root fingerprints match the first preparation exactly. No score-dependent
selection or changed training allocation occurred. Seed 1901's corrected run uses the exclusive
directory `target/layout-direct-budget-seed-1901-02`.

At the runner handoff check, seed 1901 had completed both 26-episode baseline evaluations and
six cumulative updates (12,288 training actions). The six persisted before/after state hashes
form an uninterrupted chain. Its first full capsule reproduces the 2,048-action update exactly,
including Adam and model state; gradient SHA-256 is
`b9c7f3416ac79d9eb73d499fbc86f2c5f0ddd1f73f43a550cfae849f555aa8d1`.
This is implementation evidence, not an efficacy result. Seeds 2901 and 3901 have also started
with the identical frozen evaluation set in their respective `layout-direct-budget-seed-*-01`
directories. All three commands remain live at this handoff; no final result is available.

Subsequent checkpoint: all three seeds have passed 64 updates, exact replay and their prospective
evaluations, then continued beyond 64 with uninterrupted state chains. See
`budget-curve-64-report.md` for the verified early outcomes and direct geometry inspection.
This is not completion of the declared 256/1024 allocation.

All three 256-update checkpoints and their fixed evaluations are now verified. The paired
development endpoint against initial falls from 0.18056 at 64 to 0.09722 at 256; direct
256-versus-64 comparisons are approximately flat (34 wins, 36 losses, two ties). The full
target remains unchanged. See `budget-curve-256-report.md` for complete counts, work/futility,
construction exposure and the common-scale actual-geometry inspection. The unchanged runs
continue to 1024; neither a final budget curve nor production readiness is established.

## Verification and boundaries

- Thirty-five Python tests pass, including exact update replay/credit references and ten new
  budget tests for allocation, RNG isolation, matching, strict effort/audits, family exclusion,
  no-fallback generation, persisted episode consistency and the startup barrier.
- Fifteen direct Node tests, `cargo check --workspace` and `cargo test --workspace` pass;
  two existing Rust tests remain ignored. No Rust source or dependency edits.
- Manual CLI preparation, corrected multiworker evaluation, continuous training and the first
  full update replay execute successfully.
  Full cumulative results, persisted final analysis and direct geometric inspection are pending;
  passing harness tests is not a model-performance result.
- Reviewed against the accepted constructive-planner design and AGENTS policy: complete
  connected construction, immediate routing, immutable valid best in area/tiles/turns order,
  unchanged external-boundary rules and no exact-solver fallback. No production API cutover.
- Existing user changes and deferred untracked choice-fit drafts are not part of this work unit.

## Commands

```sh
uv run --frozen --project docs/benchmarks/layout-learning python docs/benchmarks/layout-direct/budget_curve.py prepare \
  --config docs/benchmarks/layout-direct/budget-curve.json --output-directory target/layout-direct-budget-evaluation-02
uv run --frozen --project docs/benchmarks/layout-learning python docs/benchmarks/layout-direct/budget_curve.py train \
  --config docs/benchmarks/layout-direct/budget-curve.json --seed 1901 \
  --evaluation-set target/layout-direct-budget-evaluation-02/evaluation-set.json \
  --evaluation-sha256 a4d336b102be55f3deb7ebdacd7e10e90bcb4194076c2c74a9e6f5bba1e0acc2 \
  --output-directory target/layout-direct-budget-seed-1901-02
```

Do not rerun a live command or reuse an output directory. Seeds 2901 and 3901 use the same frozen
evaluation artifact and their own exclusive directories; they are separate initializations and
training streams, not resets within seed 1901's run. The declared final allocation is 1024
updates per seed. A partial checkpoint or operational failure cannot stand in for that curve.
