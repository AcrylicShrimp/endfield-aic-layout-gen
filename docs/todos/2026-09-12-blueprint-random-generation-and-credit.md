# Fresh blueprint inputs and advantage scaling

Status: in progress.

Context: restore the previously required per-episode random products, rates and
initial layouts before conducting the user-requested advantage comparison.
Contract: [design](../designs/2026-09-12-blueprint-random-generation-and-credit.md).

## Completion criteria

- Explicit fresh input supply for normal training; fixed fixtures restricted to diagnostics.
- Per-stream immutable originals, durable generation provenance, authentic restore and audit.
- Real random-generation/continuation/evaluation-isolation readiness passes.
- Required tests and artifact checks pass; commit the restoration milestone.
- Register and run the matched three-seed advantage comparison after readiness.
- Audit results, verify tracking, write bounded findings/provenance and commit.

## Activity

- 2026-09-12: traced regression to the blueprint learner's shared fixed Runtime input.
  Confirmed the preceding generator remains available with four external product
  families and randomized complete valid initial geometry. No new training started.
- 2026-09-12: restored explicit fresh inputs and per-stream original ownership.
  Real 64-action readiness passed after process restoration: 24 generated
  episodes, four products, 20 requests, 23 geometries, six exact update replays
  and 96 full-action evaluation cases. Python 420 tests (seven skipped), Rust
  check and 564 tests (five ignored) passed. Advantage comparison remains next.
