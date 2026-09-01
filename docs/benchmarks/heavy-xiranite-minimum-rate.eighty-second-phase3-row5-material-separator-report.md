# Heavy Xiranite Phase 3 Row-5 Material Separator

## Result

The exact row-5 separator sharply narrows junction child E but still does not find a first feasible
witness within the five-second case budget. Fifteen of the sixteen canonical first-crossing cases
are proven infeasible. The only unresolved child is case 1:

```text
inherited selected-material prefix: 48 -> 64 -> 80 -> 81
only unresolved first row-5 crossing:              81 -> 97
```

The control root already fixes `Q(81 -> 97)` true while leaving `Q(80 -> 96)` unresolved. The
fourteen cases after case 1 therefore contradict that existing root fact through their canonical
prefix exclusions. The new nontrivial result is case 0: it requires 1.34 seconds of search to prove
that `Q(80 -> 96)` is impossible in E. Within junction child E, every feasible solution must
therefore belong to case 1. Case 1 nevertheless consumes its complete five-second budget without a
witness or proof.

This result satisfies the predeclared stop rule. It is the last row-local partition. The next
experiment should change scale and test an exact iterative decomposition with solver-generated
feedback, rather than manually extending the selected route one row at a time.

## Exact Experiment Contract

The experiment continues the unresolved E child of the exact junction partition for
`network:pipe:item-liquid-xiranite-poly`. The inherited fixed dimensions are `16 x 16`; these are a
controlled diagnostic fixture, not a production default or game limit.

The inherited exact selected-material facts are:

```text
source continuation:     48 -> 64
row-4 first crossing:    64 -> 80
junction E continuation: 80 -> 81
```

The complete horizontal separator after row 5 contains the sixteen south-directed arcs from
`80 -> 96` through `95 -> 111`. For each arc `e`, `Q(e)` means that the arc is selected and carries
item code 5. Canonical child `i` requires `Q(i)` and excludes every `Q(j)` where `j < i`.

The children are non-empty as predicates, pairwise disjoint, and exhaustive inside junction child
E. They do not assume that only one separator crossing exists. Later crossings, north recrossings,
splitters, convergers, cycles, bridges, flow magnitudes, placement, rotation, ports, every other
route, and every other commodity network remain solver decisions. Junction child S remains an
independent unresolved sibling and is not closed by any E-local result.

The encoding adds no selector variables. Child `i` adds two unary constraints for `Q(i)` and one
binary exclusion clause for every earlier predicate.

## Controlled Result

All authoritative and observation runs agree. Every exact-cover, raw-certificate, inherited-
certificate, fixture, model-family, hidden-domain, root-restriction, and parent-child evidence gate
passes. There are no invalid witnesses or proof conflicts.

| Case | First row-5 crossing | Outcome | Root conflict | Search | Decisions | Backtracks | Conflicts | Learned clauses | Solver propagations |
| ---: | --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Control | unrestricted | Unknown | no | 5,007 ms | 56,605 | 4,009 | 4,008 | 4,008 | 5,236,394 |
| 0 | `80 -> 96` | Proven infeasible | no | 1,340 ms | 11,578 | 635 | 636 | 636 | 1,237,303 |
| 1 | `81 -> 97` | Unknown | no | 5,007 ms | 52,179 | 3,700 | 3,699 | 3,699 | 4,845,324 |
| 2 | `82 -> 98` | Proven infeasible | yes | 124 ms | 0 | 0 | 1 | 1 | 207,786 |
| 3 | `83 -> 99` | Proven infeasible | yes | 127 ms | 0 | 0 | 1 | 1 | 208,449 |
| 4 | `84 -> 100` | Proven infeasible | yes | 126 ms | 0 | 0 | 1 | 1 | 208,428 |
| 5 | `85 -> 101` | Proven infeasible | yes | 127 ms | 0 | 0 | 1 | 1 | 208,428 |
| 6 | `86 -> 102` | Proven infeasible | yes | 128 ms | 0 | 0 | 1 | 1 | 208,362 |
| 7 | `87 -> 103` | Proven infeasible | yes | 122 ms | 0 | 0 | 1 | 1 | 208,362 |
| 8 | `88 -> 104` | Proven infeasible | yes | 124 ms | 0 | 0 | 1 | 1 | 208,362 |
| 9 | `89 -> 105` | Proven infeasible | yes | 120 ms | 0 | 0 | 1 | 1 | 208,362 |
| 10 | `90 -> 106` | Proven infeasible | yes | 123 ms | 0 | 0 | 1 | 1 | 208,362 |
| 11 | `91 -> 107` | Proven infeasible | yes | 129 ms | 0 | 0 | 1 | 1 | 208,362 |
| 12 | `92 -> 108` | Proven infeasible | yes | 89 ms | 0 | 0 | 1 | 1 | 208,362 |
| 13 | `93 -> 109` | Proven infeasible | yes | 83 ms | 0 | 0 | 1 | 1 | 208,362 |
| 14 | `94 -> 110` | Proven infeasible | yes | 92 ms | 0 | 0 | 1 | 1 | 208,368 |
| 15 | `95 -> 111` | Proven infeasible | yes | 86 ms | 0 | 0 | 1 | 1 | 208,369 |

The result totals zero validated feasible cases, fifteen proven-infeasible cases, one `Unknown`
case, and zero invalid cases.

Compared with the unrestricted control, surviving case 1 reduces branch decisions by 7.8%,
conflicts by 7.7%, and solver propagations by 7.5%. Those counter reductions do not change the
operational result: both runs consume the full cutoff and neither produces an incumbent or proof.

## What the Fifteen Proofs Mean

The sixteen cases cover every possible first selected-material south crossing of row 5 inside E.
The unrestricted control already fixes `Q(81 -> 97)` at root. Cases 2 through 15 exclude that
earlier true predicate as part of their canonical definition, so those fourteen root proofs do not
represent fourteen independent route discoveries. Case 0 instead permits the already-fixed
`81 -> 97` crossing and additionally requires `80 -> 96`; its 1.34-second search proof supplies the
new exact fact `not Q(80 -> 96)`.

Together, the complete partition and proofs show that any feasible E solution must use case 1. Its
certified root prefix is:

```text
48 -> 64 -> 80 -> 81 -> 97
```

This is a solver-derived result, not a hand-written route choice. The first part, `81 -> 97`, was
already propagated by the unrestricted E model. The experiment adds the nontrivial proof that the
same solution cannot also take the earlier south crossing `80 -> 96`. Later row-5 crossings after
`81 -> 97` remain legal.

The result does not prove that case 1 is feasible. It also does not prove that E or the inherited
parent is feasible, because case 1 remains `Unknown` and sibling S is separately `Unknown`.

## Model Delta

The control contains 63,385 variables, 161,638 constraints, 618,984 incidences, and 242,663
placement-routing incidences. Every child preserves all variables and all placement-routing
incidences.

| Model | Variables | Constraints | Incidences | Placement-routing incidences |
| --- | ---: | ---: | ---: | ---: |
| Control | 63,385 | 161,638 | 618,984 | 242,663 |
| Case 0 | 63,385 | 161,640 | 618,986 | 242,663 |
| Case 1 | 63,385 | 161,641 | 618,988 | 242,663 |
| Case 15 | 63,385 | 161,655 | 619,016 | 242,663 |

Child `i` adds exactly `2 + i` constraints and `2 + 2i` incidences. All non-material-separator
constraint families are byte-for-byte equal to the control metrics. The measured deltas therefore
match the declared controlled axis.

## Why Row-Local Splitting Stops Here

The sequence of exact diagnostics has now established:

1. the selected source and boundary endpoint choices can be isolated exactly;
2. row 4 is forced at root to `64 -> 80`;
3. cell 80 can continue east or south, but neither complete child resolves in five seconds;
4. inside the east child, row 5 is narrowed to `81 -> 97`, yet that sole survivor still does not
   resolve in five seconds.

Another row separator could continue discovering the route one strip at a time. That would remain
exact, but it would repeat a manual decision-tree construction around one commodity while leaving
placement, ports, other commodities, shared occupancy, and downstream flow coupling intact. The
new experiment itself takes only 24.7 seconds, while reconstructing the entire accepted parent
chain makes the CLI take 403.7 seconds. This is no longer a productive scale for cliff diagnosis.

The useful finding is more general: the exact model already derives some route facts at root, and a
small isolated branch can derive one additional reusable fact that the monolithic five-second run
does not surface as a persistent parent cut. A hand-authored sequence of such branches is not a
production solution, but solver-generated exact cuts could be.

## Proposed Exact Decomposition Direction

The next candidate is an exact iterative master/subproblem experiment. It must remain separate from
the authoritative joint baseline until measured and explicitly approved as a replacement.

The experiment should not perform one-way placement followed by routing. Instead:

1. a master model chooses the shared high-level interface state, initially placement, rotation,
   ports, dimensions, and any cheap exact necessary connectivity state;
2. an exact routing subproblem checks whether that interface admits all belt and pipe networks;
3. if routing is proven infeasible, the subproblem returns a sound nogood over a verified subset of
   interface decisions;
4. the master adds that cut and proposes another state;
5. a feasible route returns an incumbent and objective upper bound; master lower bounds and exact
   dimension or transport-budget cases are used to close the remaining optimality gap.

This is progressive without freezing an earlier layout. Every iteration may move every facility
and change every port. A cut removes only an interface combination whose routing infeasibility was
proved, so no legal solution or better objective may be discarded.

The critical research question is cut strength. A full-assignment nogood is sound but may remove
only one bad placement. A smaller routing-infeasible core could remove a large family of bad states,
but every literal removed from the core must be rechecked exactly. The first controlled experiment
should therefore measure:

- master time, routing-subproblem time, and iteration count;
- full nogood arity and minimized proven core arity;
- how many later master candidates each cut eliminates;
- first-feasible time and incumbent objective;
- lower and upper bounds after each iteration;
- whether repeated cuts converge or merely enumerate complete assignments.

The present result supplies a concrete prototype cut inside its full inherited context:

```text
not Q(80 -> 96)
```

The full master/subproblem loop is not yet the next implementation. Cut reuse must first be shown
in a smaller controlled A/B experiment.

## Smallest Next Experiment: Guarded Core Extraction and Replay

The next slice should extract a sound conditional core from the already proven-infeasible E/case-0
leaf and replay that core into the unchanged joint parent.

1. Keep the unchanged joint E control as the baseline and retain sibling S in the proof tree.
2. Serialize the case-0 premises as native predicates: dimensions, facility coordinates and
   rotations, port choices, boundary and endpoint choices, source continuation, row-4 and junction
   state, and `Q(80 -> 96)`.
3. Delete one premise at a time and rebuild the exact model. A premise may be removed from the core
   only when the enlarged problem is **ProvenInfeasible**. `Unknown` or a feasible witness retains
   the premise. The result is sound and proof-carrying, although not necessarily minimum.
4. Add one guarded nogood, `not(all retained core predicates)`, to the otherwise unchanged joint
   parent. Compare its model delta, root domains, five-second outcome, decisions, conflicts, and
   propagations against the control.
5. Stop if the core remains close to the full assignment or replay merely reproduces the surviving
   child's approximately 7.5% counter reduction. Advance to a two- or three-iteration
   master/routing-subproblem prototype only if the core shrinks materially and eliminates multiple
   exact parent candidates.

Timeout and counter improvements are never proof that a premise is unnecessary. Only a completed
infeasibility proof may shrink the guarded core. The cut must remain conditioned on every retained
premise and scoped to E; it cannot be promoted to sibling S or the general model.

The Phase 3 fixed-dimension fixture is sufficient for this feasibility experiment. It should not
attempt the complete Heavy Xiranite objective hierarchy until reusable exact feedback is
demonstrated.

## Improvements Preserved in the Exact Baseline

The measured run retains:

1. shared belt and pipe physical layers;
2. factored placement and port variables;
3. canonical physical occupancy coupled bidirectionally to transport occupancy;
4. external terminals represented inside commodity-network routing;
5. exact parallel dimension cases and proof-derived bound sharing;
6. possible-graph connectivity propagation;
7. event-driven unique-support and local-continuation propagation;
8. guarded positive-item intersection propagation;
9. exact placement, rotation, port, endpoint, boundary, and residual-tuple partitions;
10. exact sparse legal external-boundary domains;
11. exact source and endpoint continuation controls;
12. exact row-4 separator and material-junction partitions;
13. ordered multi-separator root snapshots and raw build certificates;
14. fail-closed inherited-certificate identity across control and child models;
15. the new complete row-5 partition with no selector variables.

No layout, route, corridor, path order, port choice, or other heuristic is introduced.

## Independent Review

Before the release run, three independent reviews examined proof soundness, experiment isolation,
and next-strategy boundaries. One review blocked the implementation because child models did not
compare inherited raw certificates against the control, row-5 CLI mode silently ignored a
contradictory parent row, and central fail-closed paths lacked negative tests. The implementation
was corrected to compare every inherited boundary, continuation, row-4 separator, and junction
certificate; require parent separator row 4; and exhaustively test all 65,535 non-empty crossing
subsets together with ordered-stack, family-drift, and root-infeasible gates. Re-review passed.

All three post-run reviews pass. They confirm that the complete E-local cover supports exactly this
conditional statement: if E has a feasible solution, its first row-5 selected-material south
crossing is `81 -> 97`. They also confirm that E itself and sibling S remain unresolved and that
later row-5 crossings remain legal.

The experiment review verified all measurements, proof locations, model deltas, artifact counts,
and fail-closed gates. The strategy review rejected another row separator and narrowed the next
slice from a full master/subproblem implementation to guarded core extraction and replay. This
tests the required reusable-cut mechanism before committing to a larger alternative architecture.

## Timing and Artifacts

The row-5 experiment took 24,681 ms:

- authoritative control: 5,569 ms wall;
- observation control: 5,563 ms wall;
- authoritative child wave: 6,765 ms wall;
- observation child wave: 6,781 ms wall.

The complete chained diagnostic took 403,701 ms because the CLI reconstructs every accepted parent
portfolio before the new wave.

Artifacts are stored in `/tmp/aic-phase3-row5-separator.Vgysss`:

- `summary.json`, `stdout.json`, and self-contained `summary.html`;
- authoritative and observation control wireframes;
- authoritative and observation artifacts for all sixteen children.

## Verification

Final verification passed:

```text
cargo fmt --all
cargo check --workspace
cargo test --workspace
cargo build --release --workspace
git diff --check
```

This includes 34 main CLI tests, 3 prior-terminal CLI tests, and 304 data-library tests, as well as
the complete release workspace build and final diff check.
