# Fresh blueprint inputs and advantage scaling

Status: complete for fresh-input restoration and the registered advantage screen.
Results: [final report](../reports/2026-09-12-blueprint-fresh-input-advantage.md).

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
- 2026-09-12: registered 512 updates per condition and three learner seeds.
  Added centered-only credit with unchanged value targets, exact nonempty-Adam
  and self-imitation replay tests, a shared matrix supervisor, paired-input
  result analysis and a dedicated W&B view helper. Six-arm preparation passed
  locally; the server readiness matrix precedes main training.

- 2026-09-13 KST: completed all six registered 512-update arms after six-arm
  readiness. All arms consumed the same 2,048 fresh inputs (194 requests,
  1,723 geometries); all 24,768 evaluations completed 64 actions. Final tail
  audits and 774-point tracking readback passed. Centering wins the primary
  final/late means, but its 25-case geometry-exclusion late mean loses; both
  modes show rising aggregate block means after an early drop. Preserved raw
  server evidence and local metadata/provenance; no additional long run.
- 2026-09-13 KST: recorded general continuity and bounded isolated verification
  rules in `AGENTS.md` (`41a9707`, `a05fa9f`). Existing blueprint execution still
  lacks the previous persistent collector-worker capability and attaches costly
  audits to normal completion. These are open migration obligations documented
  in the final report, not resolved capabilities or newly optional enhancements.
  This bounded experiment's completion does not certify the full learner cutover.
