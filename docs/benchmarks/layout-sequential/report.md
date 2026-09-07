# Expanded-candidate sequential RL baseline

Status: measured pilot, not a production-ready or general-factory learner. No further budget
expansion is planned for this fixed-root experiment. Continue with dynamic environments and
an end-to-end learned-policy evaluation rather than more operator enumeration.

## What ran

Actual online policy-gradient training: 32 updates, four 64-action trajectories per update,
8,192 sampled transitions. A 16,321-parameter graph policy scores a pre-enumerated finite
candidate list. It is not a direct action proposer. Training chooses among 52 fixed small
roots, with the strong Heavy Xiranite target in every fourth update; it does not generate a
fresh recipe combination on reset. Only a subset of the available small roots was sampled.

17 update groups have nonzero terminal lexicographic comparison signals. The other groups
have no task signal; the explicit 0.001 entropy term may still update weights. 15,453 weight
elements changed. Exact JSON checkpoint restore preserves predictions. These facts demonstrate
that optimization ran, not that a useful or general strategy was learned.

Initial checkpoint SHA-256: `0605f95eceb93312af4d0baee8498bdd9e1f1c9768bb2b7109603ba6aa271cdb`.
Trained checkpoint SHA-256: `d6a0a0a4381337aaec9f70459059acf5aa0b1f5105ce8ddbf60eb5d2b744e1f0`.
Full run: `target/layout-sequential-train-01/report.json`; compact hash-checked results are
committed in `results.json`. Source/report/trajectory provenance is recorded. Total elapsed
time is 980.2 seconds on CPU, including training and evaluation.

## Matched initial-weight comparison

Both models use the same roots, stochastic evaluation seed schedule and action budgets.
All 40 evaluation episodes finish their actual 128 or 1,024 proposals, without deadline or
domain truncation. Every retained output passes a fresh independent complete-factory audit;
every training episode's retained output is also freshly audited. Sealed-test evaluations: zero.

| Development action budget | Trained wins/ties/losses on 9 roots | Initial mean area/root area | Trained mean area/root area | Initial/trained futile rate |
| --- | --- | --- | --- | --- |
| 128 | 1 / 8 / 0 | 90.046% | 90.046% | 77.778% / 78.385% |
| 1,024 | 5 / 2 / 2 | 85.719% | 84.733% | 78.570% / 78.472% |

One training seed, one evaluation seed, and nine fixed development roots are insufficient to
establish reliable generalization or statistical learned benefit. The result is a weak pilot
signal, not a passed readiness gate. Generation/action freedom benefits must not be attributed
to training: the initial-weight model already improves several roots using the same executor.

Heavy Xiranite remains exactly `2132 / 392 / 147` for both controls at both budgets, with
29.6904% empty XY area. The 1,024-step futile rates are 80.176% initial and 78.418% trained.
Seven of eight target training groups have no task signal; only the first finds a one-tile
wiring improvement on one trajectory. Neither training nor long evaluation reduces target area.
No new dense target geometry was produced; the previously audited baseline remains the witness.
This does not prove the requested 1% goal impossible.

The trained target evaluation takes 255.4 seconds: 201.4 seconds candidate generation/static
screening, 13.4 seconds features, and 2.5 seconds repair-kernel work. Preprocessing dominates.
Episode wall time includes policy/batch waiting; do not treat it as isolated per-root latency.

## Scope, invariants and next action

The constructor/design policy comparison preserves the original production request, pipe-first
repair, independent validation, exact score ordering and a separate best valid incumbent.
Private exploratory transitions may worsen, but cannot replace the retained best output.
There is no global exact fallback and no changed production command. The local executor keeps
the old current-extent and external-anchor restrictions; those are not general game limits.

Per the user's clarification, initial capability and learning checks are useful pilots, not
the main objective. Stop extending this pilot. Build a fresh-factory generation -> validated
reset -> learned actions -> multi-step training -> independent evaluation path. Candidate
selection versus direct proposal is a design choice in service of the goal, not the goal itself.

## Verification and reproduction

- Eight new JavaScript tests cover expanded feature/mask accounting, delayed valid deterioration,
  compound validation, retained state on failure/deadline, terminal padding and result accounting.
- Three new Python tests cover configuration, inactive trajectories and task-gradient/restore.
- The 23 production metric/capability/interaction tests and 16 earlier RL/RL-next JavaScript
  tests pass. Workspace check/test pass; no Rust files were edited.

```sh
uv run --frozen --project docs/benchmarks/layout-learning python docs/benchmarks/layout-sequential/train.py --roots docs/benchmarks/layout-sequential/plan.json --config docs/benchmarks/layout-sequential/train.json --output-directory target/layout-sequential-train-01
node docs/benchmarks/layout-sequential/results.mjs --report target/layout-sequential-train-01/report.json --output-directory target/layout-sequential-results-01
node --test docs/benchmarks/layout-sequential/*.test.mjs
uv run --frozen --project docs/benchmarks/layout-learning python -m unittest discover -s docs/benchmarks/layout-sequential -p 'test_*.py'
cargo check --workspace
cargo test --workspace
```

Use new output directories for reproduction. Existing artifacts are never overwritten.
