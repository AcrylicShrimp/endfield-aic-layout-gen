# Minimum physical chain length within the minimum extent

Status: verified improvement slice; the full goal remains active.

## Result and visual finding

The complete 59-facility heavy-xiranite layout keeps **91x75 (6,825 area)** while physical transport
tiles fall **1,789 -> 1,603**, a **10.40% reduction**. Turns remain 261 and components remain fifteen.
All 131 original recipe terminals and seven real external obligations remain independently validated.
Area still ranks first: no larger result is selected merely for using fewer tiles or components.

Personally inspected the actual result before and after review fixes. The visible F55/F58 defect
is corrected: both move from y=3 to y=37 while F54 remains at y=42. The long empty local span
collapses without recipe changes, hardcoded label rules or relaxed attachment geometry. The full
render still contains a substantial upper-left void; this is not satisfactory final compactness.

Final page: `http://100.114.24.86:8765/bound-interconnect-closure-minimum-length-final.html`.
Native image: `target/shared-results/bound-interconnect-closure-minimum-length-final.png`.
Six viewport/theme checks and full-factory zoom/drag/Fit checks pass.

## Method and exact scope

The primary difference-constraint solve still finds minimum extent. At that extent, reversed
inequalities give feasible upper bounds. Each remaining bounded integer coordinate is represented
by prefix thresholds. Difference inequalities become implications, and chain-length coefficients
become finite source/sink capacities. An iterative level-graph max flow solves minimum closure.
Every prefix, bound, difference inequality and primal/dual cost equality is checked. Objective
arithmetic uses BigInt before converting a safely representable final result back to JSON numbers.

Parallel physical chains are deduplicated by transport layer and geometric endpoints; rates are
not objective weights. The actual lowered tile-count change must equal the predicted chain-cost
change. This certifies the physical objective rather than merely a proxy distance. Rigid catalog
ports, component identities, boundary sides, direction and rates are unchanged. See
`length-contract.md` for the reduction and scope: minimum extent then minimum tiles for fixed
perpendicular coordinates, order, coincidences and topology, not global factory optimality.

The previous extent-only final selection is replaced internally; there is no compatibility mode
or fallback on a resource limit. Its least solution remains necessary as the primary bound and
secondary feasibility certificate. Diagnostic node/edge/work limits reject a candidate explicitly;
the CLI retains its independently validated complete incumbent.

## Multi-start comparison

Four complete original-graph seeds were tested with both axis orders. All **38 axis candidates**
validate. Current-source results and report hashes are frozen in `length-portfolio.json`.

| Starting complete area | XY final area / tiles | YX final area / tiles |
| ---: | ---: | ---: |
| 6,825 (previous extent-only result) | 6,825 / 1,603 | 6,825 / 1,603 |
| 8,415 (shared-column source layout) | 6,825 / 1,603 | 6,825 / 1,603 |
| 9,416 (layered crossings) | 7,315 / 1,710 | 7,469 / 1,728 |
| 9,856 (copy-optimized earlier layout) | 7,238 / 1,683 | 7,392 / 1,701 |

The older seeds do not improve the incumbent. These experiments are evidence of different fixed
neighborhoods, not a proof that all other starts or relative orders are exhausted. The final
selected report is `length-matrix.json`; previous matrices remain historical evidence.

The finalized selected CLI run took **0.907 s**, including four axis passes, incumbent/candidate
validation, reads and diagnostic/HTML output, excluding prior factory generation and final report
serialization. All eight runs took 0.86-1.20 s individually; these are single observations, not
controlled median benchmarks. Initial actual threshold graphs have roughly 500-900 nodes and
1,200-2,200 edges. Detailed model/extent/length/lowering timing and graph/work counts are persisted
per candidate. No Rust rebuild or new dependency is needed for the experimental algorithm.

## Verification and reviewed fixes

- 72 related Node tests pass, including 1,000 independently enumerated bounded integer systems,
  the existing 500 difference systems and 2,000 runtime catalog port/axis cases.
- All eight complete-factory positive/negative audit expectations pass (`length-controls.json`).
- `cargo check --workspace` and `cargo test --workspace` pass: 524 tests, two existing ignores.
  No Rust source or dependencies changed. Geometry uses the existing release witness probe.
- Independent proof review confirmed the closure reduction and deduplicated physical-chain cost.
  An independent Edmonds-Karp implementation reproduced actual X/Y flow values 181/234. The main
  agent independently reran the tiny exhaustive tests, final factories and physical tile certificates.
- Both independent reviews identified large-integer certificate issues. Locally reproduced fixed
  costs reported as 1 instead of 2 and 4 instead of 3, plus a one-threshold range near the maximum
  safe integer selecting the wrong endpoint. BigInt dot products and relative threshold indices
  fix these, with permanent tests. Original chain cost is summed from geometric lengths rather
  than cancellation-prone large absolute coordinates. Current factory geometry was unaffected:
  all four selected-run witness hashes remain identical after the fixes.
- The diagnostic review found uncounted threshold construction work and accepted NaN budgets.
  Both were verified locally; construction/flow work now share the explicit budget, noninteger
  budgets are rejected, and self-implication stress has a regression test.
- Validator timeouts previously sent only SIGTERM. The owned process now receives SIGKILL after
  a bounded grace period, and a timeout cannot accept a report even if one exists. A real child
  that installs a SIGTERM handler is forcibly terminated in the regression test. Process status
  is saved separately in `validator-process.json`; ordinary complete validator output is flushed.
  This change is scoped to the new compaction runner, not a claim about every historical harness.
- No reviewer claim was used as an unverified game or pruning rule. No further correctness bug
  was confirmed. All final portfolio source hashes match the current implementation.

Compared with the constructive planner design, the constructor remains connected/transactional
and improvement commits require complete independent validation. There is no global joint solver,
caller-supplied production canvas, fake external material or speculative game rule.

## Next area gate

The fixed-neighborhood solver now handles the secondary objective, but strict single-pass score
selection can discard a different equal-score embedding that would help the perpendicular pass.
Next investigate alternate optimal cuts (least and greatest threshold closures), apply the other
axis privately, and select only after the complete multi-pass candidate improves. Thereafter,
bounded changes to relative region order are another distinct neighborhood. These are proposed
experiments, not implemented improvements or global optimality claims. Preserve the complete
6,825-area/1,603-tile incumbent. Final compactness, additional complete recipes, cleanup and the
actual recipe/rate-to-layout web service remain unfinished.
