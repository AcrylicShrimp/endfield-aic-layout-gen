# Heavy Xiranite Phase 3 Guarded-Core Shrinking

## Result

Sequential proof-only shrinking reduces the accepted Phase 3 guarded conjunction from 30 native
predicates to 9 while preserving an independently verified infeasibility proof.

The experiment removes 21 predicates, or 70% of the original conjunction. Every removal is
authorized by a fresh exact solve that proves the enlarged candidate model infeasible. No
placement, port, routing, boundary, or flow predicate is removed on timeout alone.

The final nine-predicate conjunction is independently rebuilt and proven infeasible in 3,588 ms
of search. A separate root-observation build produces the same proof and the same search counters.
All certificate, model-identity, exact-delta, unrestricted-boundary, and root-predicate gates pass.
The result is `Completed` with `interpretation_blocked = false`.

This is a deletion-minimal result only with respect to the ordered one-pass procedure and the
five-second per-candidate proof budget. The nine retained predicates are not proven necessary:
removing any one of them produced `Unknown` within that budget, not a feasible counterexample.

## Controlled Contract

The run first repeats the accepted 30-atom native replay gate. It then visits the original atoms
in stable order. For each atom it:

1. removes exactly that atom from the current retained conjunction;
2. rebuilds a fresh unrestricted exact joint placement-and-routing model;
3. checks the exact native certificate set and model delta;
4. removes the atom only after `ProvenInfeasible` with all fail-closed gates satisfied;
5. retains the atom for `Unknown` or a validated feasible witness;
6. blocks the experiment on invalid evidence, certificate drift, model drift, or boundary-domain
   restriction.

The deletion wave does not use root observation. After the wave, the final retained conjunction is
rebuilt twice: once for the authoritative proof and once for independent root evidence.

## Shrinking Summary

| Metric | Result |
| --- | ---: |
| Initial predicates | 30 |
| Removed with exact proof | 21 |
| Retained after timeout | 9 |
| Reduction | 70% |
| Validated-feasible deletion candidates | 0 |
| Invalid or blocked candidates | 0 |
| Sum of candidate build time | 15,374 ms |
| Sum of candidate search time | 104,466 ms |
| Sum of candidate wall time | 122,070 ms |
| Shrinking stage wall time | 130,397 ms |
| Complete parent-chain and experiment wall time | 542,227 ms |

The repeated initial gate returned `Go`: the full conjunction was proven infeasible, its
independent observation agreed, and the atom-free unrestricted control remained `Unknown` after
five seconds. Reconstructing the complete accepted parent chain took 411,829 ms of the total wall
time.

## Final Retained Conjunction

| Category | Predicate |
| --- | --- |
| Placement | Liquid-xiranite-poly input facility at `(1, 0)`, rotation `180` |
| Placement | Xiranite-powder input facility instance 1 at `(8, 5)`, rotation `0` |
| Facility port | Supply terminal `7565...` uses `output-belt-4` |
| Facility port | Supply terminal `dc9b...` uses `output-belt-0` |
| Facility port | Demand terminal `8984...` uses `input-belt-0` |
| Facility port | Supply terminal `9cc7...` uses `output-pipe-3` |
| Facility port | Demand terminal `f959...` uses `input-pipe-2` |
| Route topology | Liquid-xiranite-poly arc `80 -> 81` is selected |
| Route topology | Liquid-xiranite-poly arc `80 -> 96` is selected |

The exact terminal and entity IDs remain serialized in `summary.json`; abbreviated IDs above are
only for readability. The retained pattern contains no fixed used width, used height, external
boundary key, material item label, or material flow magnitude. It also retains only two of the
four original placements, five of the fifteen facility-port assignments, and two of the eight
route/material predicates.

## Candidate Outcomes

| Original atom range | Proven removable | Retained `Unknown` |
| --- | ---: | ---: |
| Used dimensions | 2 | 0 |
| Placements | 2 | 2 |
| Facility ports | 10 | 5 |
| External boundary key | 1 | 0 |
| Route/material state | 6 | 2 |
| **Total** | **21** | **9** |

Successful deletion proofs took between 623 ms and 4,665 ms of search. Every retained candidate
consumed the full five-second search budget. No candidate returned a feasible witness, so this run
does not distinguish a truly necessary predicate from a harder but still redundant predicate.

## Final Independent Proof

| Run | Outcome | Build | Search | Decisions | Backtracks | Conflicts | Learned clauses | Solver propagations |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Final nine-atom authoritative | Proven infeasible | 540 ms | 3,588 ms | 12,755 | 954 | 955 | 955 | 3,109,699 |
| Final nine-atom root observation | Proven infeasible | 520 ms | 3,513 ms | 12,755 | 954 | 955 | 955 | 3,109,699 |

The final model has 64,471 variables, 163,824 constraints, and 626,026 factor-graph incidences.
Relative to the atom-free control, the exact delta is nine unary native constraints and nine
incidences. All other model-complexity fields are identical.

## Evidence Preserved

Each deletion attempt serializes:

- the ordered candidate atom IDs;
- the complete native guarded-core and unrestricted-boundary certificates;
- outcome, termination, proof, and validation states;
- build, search, first-incumbent, and wall times;
- decisions, backtracks, conflicts, learned clauses, and solver propagations;
- variable, constraint, incidence, and complete model-complexity metrics;
- the explicit proof-authorization decision;
- a self-contained authoritative HTML rendering.

The final result additionally serializes both complete final layouts, both certificate sets, and
the independent root-domain snapshot. Shared artifact naming is covered by a focused CLI test.

## Independent Review

Three independent reviewers examined proof soundness, experiment validity, and next-strategy
sufficiency. Review findings identified and corrected:

1. incomplete proof handling for a possible empty retained core;
2. insufficient checking of the exact reindexed certificate subset;
3. missing raw per-attempt certificate and model evidence;
4. missing final authoritative and observation evidence;
5. inconsistent attempt artifact names between the writer and summary links.

After local reproduction and correction, all three reviewers returned `PASS` with no remaining
must-fix finding. The full workspace verification passes.

## Interpretation

The 70% reduction is material enough to continue the exact-feedback experiment. A learned guard
over nine semantic decisions can reject a much larger family of equivalent impossible master
states than the original 30-predicate leaf conjunction.

The result does not yet show that replaying the guard improves search. The next controlled question
is whether a single exact clause forbidding this nine-predicate conjunction reduces repeated work
without changing feasibility or objective quality. That must be measured against an otherwise
identical baseline; merely observing fewer predicates is not sufficient.

## Next Exact Experiment

Construct one guarded replay clause equal to the logical negation of the retained conjunction and
compare:

1. the unchanged Phase 3 baseline;
2. the same exact model with the single nine-literal replay clause.

Verify that the clause is posted only when the complete semantic and model-identity fixture
matches. Record model delta, root domains, first feasible time, termination, objective, decisions,
backtracks, conflicts, learned clauses, and propagations. The experiment remains diagnostic and
must not be promoted to the production orchestration path until its soundness and measured benefit
are independently reviewed.

## Artifacts

The release artifacts are outside the repository at:

```text
/tmp/aic-guarded-core-shrink.vG4iYa/summary.json
/tmp/aic-guarded-core-shrink.vG4iYa/summary.html
/tmp/aic-guarded-core-shrink.vG4iYa/attempt-00.authoritative.html
...
/tmp/aic-guarded-core-shrink.vG4iYa/attempt-29.authoritative.html
/tmp/aic-guarded-core-shrink.vG4iYa/final-core.authoritative.html
/tmp/aic-guarded-core-shrink.vG4iYa/final-core.observation.html
```

Verification:

```text
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
cargo build --release -p aic-cli --bin aic-prior-terminal-pair
```
