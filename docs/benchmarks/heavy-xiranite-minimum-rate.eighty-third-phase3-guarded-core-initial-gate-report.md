# Heavy Xiranite Phase 3 Guarded-Core Initial Gate

## Result

The initial exact replay gate passes with status `Go`.

The unrestricted sparse Phase 3 model plus the accepted 30 native predicates is proven
infeasible in 389 ms of search. The atom-free control remains `Unknown` after its full 5-second
budget. The proof therefore belongs to the accepted conjunction rather than to an accidentally
infeasible base model. The next exact experiment may perform sequential proof-only core shrinking.

This result does not prove the unrestricted Phase 3 problem feasible or infeasible. It also does
not authorize a production master/subproblem cutover. It validates only the controlled premise
replay needed before testing reusable exact feedback.

## Controlled Contract

The experiment rebuilds Heavy Xiranite minimum-rate cumulative Phase 3 with a `16 x 16` diagnostic
ceiling. Placement, rotation, facility ports, every external terminal, all belt and pipe routing,
flow, topology, capacity, occupancy, and logistics components remain decisions of the same joint
model. The ceiling is a fixture input, not a project or game invariant.

The 30 accepted predicates contain:

| Category | Count |
| --- | ---: |
| Used dimensions | 2 |
| Facility placements | 4 |
| Facility terminal ports | 15 |
| External boundary key | 1 |
| Route and material state | 8 |

The predicates are posted as native Pumpkin assumptions over existing domains. No selector
variables, layout heuristic, route heuristic, corridor restriction, or fixed candidate generator
is introduced.

## Runtime Result

| Run | Outcome | Build | Search | Decisions | Backtracks | Conflicts | Learned clauses | Solver propagations |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Full 30-atom authoritative | Proven infeasible | 499 ms | 389 ms | 1,606 | 504 | 505 | 505 | 557,584 |
| Full 30-atom root observation | Proven infeasible | 508 ms | 396 ms | 1,606 | 504 | 505 | 505 | 557,584 |
| Atom-free unrestricted control | Unknown | 508 ms | 5,011 ms | 8,386 | 289 | 288 | 288 | 2,951,062 |

The guarded-core experiment itself took 7,527 ms. Reconstructing the complete accepted parent
chain made total wall time 416,736 ms. The observation run exists only to capture root evidence;
its timing is not an independent performance comparison.

## Exact Model Delta

| Model | Variables | Constraints | Incidences |
| --- | ---: | ---: | ---: |
| Atom-free control | 64,471 | 163,815 | 626,017 |
| Full 30-atom model | 64,471 | 163,845 | 626,047 |
| Delta | 0 | +30 | +30 |

All non-guarded constraint families and every variable family are identical. The authoritative and
observation models are identical. The only control delta is exactly one unary native predicate and
one factor-graph incidence per accepted atom.

## Evidence Gates

Every fail-closed gate passes:

1. the compile-time fixture matches the exact Pumpkin `0.5.0` and formulation signature;
2. the complete workload wiring, cumulative Phase 3 `ModelInput`, and validated logistics-component
   definitions match their committed semantic fingerprints;
3. the exact ten external-terminal IDs and eight network item-code assignments match;
4. all external terminals retain the complete unrestricted sparse legal boundary-key domain in all
   three models;
5. all 30 ordered atom IDs are distinct and match the committed semantic fixture;
6. the authoritative and observation builds resolve the same native domains and certificates;
7. every accepted predicate is forced true in the independent root snapshot;
8. the model and factor-graph delta is exactly 30 unary constraints and 30 incidences;
9. neither observation nor control returns an invalid witness;
10. the authoritative full conjunction is independently proven infeasible;
11. the atom-free control is not proven infeasible.

The resulting status is `Go`, with `interpretation_blocked = false`.

## Improvements Preserved and Added

The run preserves the prior exact baseline improvements:

1. shared belt and pipe physical layers;
2. independent placement and port state;
3. canonical bidirectional facility/transport occupancy;
4. external terminals inside the commodity network;
5. sparse legal external-boundary domains;
6. possible-graph connectivity propagation;
7. event-driven unique-support and local-continuation propagation;
8. guarded positive-item intersection propagation;
9. exact dimension, placement, rotation, port, endpoint, separator, and junction diagnostics;
10. solver counters for decisions, backtracks, conflicts, learned clauses, and propagations.

This slice adds:

1. a native semantic atom representation with exact logical complements;
2. assumption posting without selector variables;
3. independent root-domain evidence for every accepted predicate;
4. exact model-family and factor-graph delta auditing;
5. complete external-terminal unrestricted-domain certification;
6. a compile-time accepted fixture that cannot be replaced by a CLI argument;
7. full workload, Phase 3, logistics-component, item-code, and exact solver-version identity gates;
8. explicit `Go`, `Stop`, and `Blocked` outcomes;
9. automatic JSON and self-contained HTML output for authoritative, observation, and control runs.

## Independent Review

Three independent reviews covered proof soundness, experiment validity, and next-strategy
sufficiency. The first two review rounds blocked the slice for a caller-replaceable fixture,
incomplete external-terminal auditing, missing independent root evidence, delayed mismatch checks,
an unchecked invalid control, missing logistics-component semantics, and an imprecise Pumpkin
version fingerprint.

All findings were reproduced and corrected. Final re-review returned `PASS` from all three
reviewers with no remaining must-fix item. Focused tests, CLI tests, workspace checks, and the full
workspace test suite pass.

## Next Exact Experiment

Run sequential proof-only shrinking over the ordered 30-atom vector. For each atom, rebuild the
same unrestricted exact model without that atom. Remove the atom only if the enlarged model is
`ProvenInfeasible`; retain it for `Unknown` or a validated feasible witness; block on invalid or
model-drift evidence. Finish with an independent proof of the retained core before constructing a
single guarded replay clause.

The first research question is whether the 30-predicate leaf proof shrinks materially. If it does
not, the feedback mechanism is too specific to remove many master candidates and should not be
promoted.

## Artifacts

The release artifacts are outside the repository at:

```text
/tmp/aic-guarded-core.xoR5xE/summary.json
/tmp/aic-guarded-core.xoR5xE/summary.html
/tmp/aic-guarded-core.xoR5xE/initial-full-core.authoritative.html
/tmp/aic-guarded-core.xoR5xE/initial-full-core.observation.html
/tmp/aic-guarded-core.xoR5xE/unrestricted-control.observation.html
```

Verification:

```text
cargo fmt --all
cargo check --workspace
cargo test -p aic-data guarded_core --lib
cargo test -p aic-cli --bin aic-prior-terminal-pair
cargo test --workspace
cargo build --release -p aic-cli --bin aic-prior-terminal-pair
```
