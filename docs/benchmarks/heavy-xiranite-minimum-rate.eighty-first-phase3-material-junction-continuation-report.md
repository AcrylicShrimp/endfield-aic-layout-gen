# Heavy Xiranite Phase 3 Material Junction Continuation

## Result

The exact two-way continuation partition at cell 80 is valid and produces measurable root pruning,
but neither child finds a first feasible witness within the five-second per-case budget. This one
junction partition is therefore insufficient to cross the current cliff; the timeout-censored data
do not prove that junction choice is irrelevant or not a contributor.

The inherited selected-material prefix is:

```text
48 -> 64 -> 80
```

Cell 80 is on the west boundary, is not either selected terminal, and has no legal selected-material
continuation back west because the incoming south arm and its opposing north arm cannot both be
used. Its complete local continuation is therefore partitioned into:

```text
E: Q(80 -> 81)
S: not Q(80 -> 81) and Q(80 -> 96)
```

where `Q(e)` means that arc `e` is selected and carries the selected item code. Child E intentionally
permits the south arc as well, so splitters remain legal. Child S excludes only selected-material use
of the east arc. Together, the children are non-empty as predicates, disjoint, and exhaustive over
the inherited parent feasible set.

## Controlled Result

All authoritative and observation runs agree. Every exact-cover, fixture, raw-certificate,
model-identity, hidden-domain, and parent-child evidence gate passes. There are no invalid witnesses
or proof conflicts.

| Case | Exact local condition | Outcome | Search | Decisions | Backtracks | Conflicts | Learned clauses | Solver propagations |
| --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Control | unrestricted after `64 -> 80` | Unknown | 5,006 ms | 54,485 | 3,853 | 3,852 | 3,852 | 5,518,098 |
| E | selected material uses `80 -> 81` | Unknown | 5,007 ms | 50,412 | 3,503 | 3,502 | 3,502 | 4,664,743 |
| S | selected material does not use east and uses `80 -> 96` | Unknown | 5,006 ms | 52,350 | 2,907 | 2,906 | 2,906 | 4,887,903 |

The exact split reduces counters relative to the unrestricted control, especially propagations in
E and conflicts in S, but the operational result is unchanged: zero validated feasible cases, zero
proven-infeasible cases, two `Unknown` cases, and zero invalid cases.

This is useful negative evidence. Choosing the first direction after cell 80 is not sufficient to
collapse the search. The unresolved work may lie farther downstream, in the junction choice together
with later decisions, or in coupling between that route and placement, ports, other commodity
networks, shared topology, capacity, and collision.

## Root Propagation

The unrestricted control root already fixes the inherited incoming material state:

| Arc | Selected | Flow | Item |
| --- | --- | --- | --- |
| `64 -> 80` | `1` | `1..4` | `5` |

Both outgoing predicates remain open in the control:

| Arc | Selected | Flow | Item domain |
| --- | --- | --- | --- |
| `80 -> 81` | `0..1` | `0..4` | `0..5` |
| `80 -> 96` | `0..1` | `0..4` | `0..5` |

Child E immediately fixes the east arc to selected, positive flow, and item 5 while leaving the
south arc unrestricted. Child S fixes the south arc to selected, positive flow, and item 5 and
posts the exact relational exclusion:

```text
selected(80 -> 81) = 0 OR from_item(80 -> 81) != 5
```

The S child does not incorrectly force the east arc to be globally unused; it only excludes the
predicate needed for this canonical child. Root snapshots confirm that each restriction reaches the
intended native route and item domains before the first delegated decision.

The inherited incoming arc is serialized with `case_index: null`; only the two actual continuation
candidates carry indices 0 and 1. This prevents a sentinel value from appearing as a fake candidate
index in the machine-readable certificate.

## Model Delta

The experiment adds no selector variables and leaves all placement-routing incidences unchanged.

| Model | Variables | Constraints | Incidences | Placement-routing incidences |
| --- | ---: | ---: | ---: | ---: |
| Control | 63,385 | 161,636 | 618,982 | 242,663 |
| E | 63,385 | 161,638 | 618,984 | 242,663 |
| S | 63,385 | 161,639 | 618,986 | 242,663 |

E adds two native unary constraints. S adds two unary constraints for the selected south predicate
and one binary exclusion clause for the preceding east predicate. The measured deltas exactly match
the contract.

## What Is and Is Not Proven

Proven inside the inherited fixed Phase 3 leaf:

- every feasible solution uses selected-material prefix `48 -> 64 -> 80`;
- every feasible continuation at cell 80 belongs to E or S;
- E and S preserve splitters, later crossings, cycles, flow magnitudes, placement, ports, all other
  routing, and every other commodity network as solver decisions;
- both children remain unresolved at five seconds;
- this immediate east-versus-south partition alone is not enough to cross the first-feasible cliff.

Not proven:

- that either child is feasible or infeasible;
- that the selected route has a unique continuation, magnitude, topology, or optimum shape;
- that route-local freedom is the only remaining blocker;
- that a longer exact route partition will necessarily improve runtime;
- that `16 x 16` is an optimum, default, or required game size.

The `16 x 16` dimensions remain a controlled diagnostic fixture, not a solver invariant. The
inherited predecessor witness uses height 16, four facilities fit from width 13, and the separate
exact width sensitivity run found the same five-second unresolved cliff at `13 x 16`, `14 x 16`,
`15 x 16`, and `16 x 16`. Production orchestration continues to enumerate exact dimensions rather
than treating this fixture as the target size.

## Next Exact Diagnostic

The smallest complete continuation test is a row-5 selected-material separator inside the
lowest-index unresolved child, E. It asks where the selected material first crosses the next
complete horizontal cut while preserving every placement, port, downstream route, flow, branch,
converger, cycle, bridge, and other-network decision. This is complete only within E; sibling S
remains separately unresolved and no E-local result closes the inherited parent by itself.

This is the final planned route-local refinement before changing diagnostic scale. The stop rule is:

- if the row-5 split produces root proofs, a witness, or a sharply smaller live child, continue from
  the measured surviving child;
- if it again yields multiple full-budget `Unknown` children with only modest counter changes, stop
  tracing one route cell at a time and target cross-network/shared-topology coupling instead.

The stop rule prevents an exact but uninformative chain of progressively longer fixed route
prefixes. It does not change solver semantics and introduces no heuristic reduction.

## Improvements Preserved in the Exact Baseline

The run retains:

1. shared belt and pipe physical layers;
2. factored placement and port variables;
3. canonical physical occupancy coupled bidirectionally to transport occupancy;
4. external terminals inside commodity-network routing;
5. exact parallel dimension cases and proof-derived bound sharing;
6. possible-graph connectivity propagation;
7. event-driven unique-support and local-continuation propagation;
8. guarded positive-item intersection propagation;
9. exact placement, rotation, port, endpoint, boundary, and residual-tuple partitions;
10. exact sparse legal external-boundary domains;
11. exact source and endpoint continuation controls;
12. the exact row-4 material separator and inherited prefix `48 -> 64 -> 80`;
13. complete raw variable-domain certificates and fail-closed fixture/model gates;
14. the new exact material-junction partition with no selector variables.

No layout, route, corridor, path order, port choice, or other heuristic is introduced.

## Independent Review

Three independent implementation reviews examined proof soundness, experimental isolation, worker
isolation, artifacts, and next-strategy implications. One review found that the report accepted a
worker count but the two child cases were always launched together. Both authoritative and
observation waves were corrected to execute in `worker_count`-bounded chunks. Re-review passed.

The soundness review traced the child predicates through native arc activation, flow conservation,
opposing-arm, item-continuity, non-bridge, and bridge constraints. It confirmed that the actual
outgoing set is exactly `[64, 81, 96]`, the inherited incoming state is certified, and no legal local
exit or terminal case is omitted. Structural unit tests are not an exhaustive tiny-model truth
table, but the native clauses and base constraints establish the exact-cover proof.

The first post-run review blocked two reporting defects. It rejected the categorical claim that the
junction was not the cliff because both children are timeout-censored, and it found that the
inherited incoming arc serialized `usize::MAX` as a fake candidate index. The conclusion was narrowed
to the supported claim, the index became optional, and the release artifacts were regenerated with
`null` for the incoming arc. All three final re-reviews pass: the fresh JSON/HTML artifacts, report
statistics, exact-cover interpretation, E-local next scope, and stop rule agree with no remaining
confirmed issue.

## Timing and Artifacts

The final material-junction experiment took 22,339 ms:

- authoritative control: 5,557 ms wall;
- observation control: 5,568 ms wall;
- authoritative child wave: 5,604 ms wall;
- observation child wave: 5,610 ms wall.

The complete chained diagnostic took 378,265 ms because the CLI reconstructs every accepted parent
portfolio before the new wave.

Artifacts are stored in `/tmp/aic-phase3-material-junction-final.Ly7VBQ`:

- `summary.json`, `stdout.json`, and self-contained `summary.html`;
- unrestricted control authoritative and observation wireframes;
- authoritative and observation wireframes for E and S.

## Verification

```text
cargo fmt --all
cargo check --workspace
cargo test --workspace
cargo build --release --workspace
git diff --check
```

Final verification passes 34 main CLI tests, 2 prior-terminal CLI tests, and 300 data-library tests,
as well as release compilation and diff checks.
