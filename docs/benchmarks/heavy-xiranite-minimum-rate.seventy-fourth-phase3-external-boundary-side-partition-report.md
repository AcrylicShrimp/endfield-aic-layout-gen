# Heavy Xiranite Phase 3 External Boundary-Side Partition

## Result

An exact four-way partition of one external final-product demand closes one real proof region but
does not yet produce the first feasible Phase 3 witness.

- north: `Unknown` after 5,006 ms;
- east: `Unknown` after 5,006 ms;
- south: `Unknown` after 5,006 ms;
- west: `ProvenInfeasible` after 155 ms;
- validated witnesses: 0;
- invalid witnesses or evidence conflicts: 0;
- interpretation blocked: false.

The selected parent root domain has 54 legal boundary keys. Its exact side domains contain
`11 / 16 / 11 / 16` keys for north/east/south/west. They are non-empty, pairwise disjoint, and
their union is exactly the parent domain. The west proof therefore removes one complete legal
region of fixed `16 x 16` tuple case 6. It does not prove case 6 infeasible because the other three
regions remain unresolved.

## What `16 x 16` Means

`16 x 16` is not an optimum, a default, or a game limit. It is the controlled diagnostic canvas
inherited from the preceding known placement: that placement requires height 16, while its fixed
facility geometry fits from width 13. The exact physical-layer formulation still creates state for
every cell in the chosen dimensions, so excess canvas remains a plausible cost source.

This experiment was run before the width sweep because it isolates one live semantic decision
inside the same canvas. The west proof shows that boundary attachment choice materially changes
the search. The next causal refinement is therefore the remaining north cells; width sensitivity
remains queued rather than ruled out.

## Exact Experiment Contract

The accepted sparse-boundary parent is reproduced with:

- exact used dimensions `16 x 16`;
- four facility placements and rotations fixed to the accepted tuple parent;
- all fifteen facility ports fixed;
- ten external boundary terminals present;
- all route cells, arcs, item states, flows, topology, capacity, collision, and logistics
  components left to the exact solver;
- feasibility-only search with independent 5,000 ms authoritative and observation budgets.

The controlled terminal is the external demand of network 1,
`network:belt:item-xiranite-enr-powder`. The selection rule fails closed unless that network has
exactly one possible supply option and that option belongs to a facility output.

Each child constructs the selected boundary-key variable directly from every legal key on one
side. It does not use a positive table as a pseudo-domain restriction: Pumpkin 0.5 does not eagerly
remove values absent from a positive-table projection. Direct sparse construction makes the side
restriction effective at root and avoids constructing routing-option literals for excluded keys.

## Exactness and Audit Gates

All gates passed:

| Assertion | Result |
| --- | --- |
| Parent sparse A/B is unblocked and tuple case 6 remains `Unknown` | passed |
| Selected supply is a unique facility-owned option | passed |
| Side domains are non-empty, disjoint, and an exact cover | passed |
| Selected declared domain, table projection, routing options, and restriction metadata equal the expected side keys | passed |
| Non-selected external certificate identities and domain counts match the parent | passed |
| Authoritative and observation certificates agree | passed |
| Selected root domain is contained in the requested side | passed |
| Four facility placements/rotations remain singleton and fixed | passed |
| Fifteen facility-port assignments remain singleton and fixed | passed |
| Controlled model contract after certified side-cardinality normalization | passed |
| No witness/proof conflict or invalid witness | passed |

The model contract does not claim identical constraint graphs. A retained selected key creates one
boundary equality literal and one terminal-option crossing guard. The audit therefore subtracts
the selected key count from exactly `boundary_terminal_variables` and `crossing_constraints`, then
requires every other `ExactModelMetrics` field and the formulation identifier to match. Raw model
scales remain visible below.

## Measurements

| Side | Keys | Outcome | Search ms | Decisions | Backtracks | Conflicts | Learned | Propagations |
| --- | ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| North | 11 | Unknown | 5,006 | 58,467 | 4,075 | 4,074 | 4,074 | 5,439,637 |
| East | 16 | Unknown | 5,006 | 50,675 | 5,067 | 5,066 | 5,066 | 5,566,464 |
| South | 11 | Unknown | 5,006 | 54,139 | 4,355 | 4,354 | 4,354 | 5,299,376 |
| West | 16 | ProvenInfeasible | 155 | 247 | 86 | 87 | 87 | 282,086 |

No child produced a first incumbent. Counter differences among the three timed-out children are not
runtime rankings; each consumed the full cutoff. The west result is qualitatively different: it
completed an infeasibility proof far below the budget.

| Side | Variables | Constraints | Incidences | Root selected keys |
| --- | ---: | ---: | ---: | ---: |
| North | 63,405 | 161,672 | 619,098 | 11 |
| East | 63,415 | 161,708 | 619,190 | 16 |
| South | 63,405 | 161,683 | 619,120 | 11 |
| West | 63,415 | 161,692 | 619,158 | 16 |

The north/south and east/west constraint counts differ even at equal cardinality because a side's
cells participate in different physical boundary and fixed-facility relations. This is expected
geometry, not an uncontrolled terminal-domain change.

The four child experiments took 34,860 ms. Total wall time was 199,692 ms because the current CLI
recomputes the entire accepted port-pair, completion, source-port, input-control, residual-tuple,
and boundary-key chain before reaching the side children. That repeated parent preparation is an
orchestration cost, not solver search evidence for a child, and should eventually be removed by a
stable parent-fixture input contract.

## Interpretation

The external final-product demand's boundary side is not a cosmetic choice. West is incompatible
with the already fixed placements, rotations, and facility ports under the complete routing model,
and Pumpkin proves that entire 16-key region impossible in 155 ms. North, east, and south still
contain enough route/item/flow freedom to consume the five-second budget without a witness or a
proof.

This is the first result in the current residual chain that turns a coarse external-terminal choice
into a completed proof region. It supports continuing the same exact partition hierarchy rather
than immediately changing the whole grid. It does not establish that the `16 x 16` canvas is cheap;
the physical grid remains large and the width experiment is still required if cell refinement does
not cross the first-feasible cliff.

## Next Exact Experiment

Partition the lowest-index unresolved side, north, into its eleven singleton boundary keys. The
eleven children are disjoint and exactly cover north:

- any validated child witness proves fixed tuple case 6 and its parent feasible;
- all eleven infeasibility proofs close north;
- mixed proof and `Unknown` results preserve only the unresolved cells;
- no location outside north is excluded by this child portfolio.

If singleton north still yields only unresolved cells without a first witness, run the complete
fixed-height width portfolio `13 x 16`, `14 x 16`, `15 x 16`, and `16 x 16`. This directly tests the
user's canvas-size hypothesis. A raw runtime `DomainId` must never be partitioned before its
semantic predicate is identified.

## Improvements Preserved in the Exact Baseline

The current formulation retains the cumulative exact work:

1. shared belt and pipe physical layers instead of one dense grid per logical line;
2. factored placement and port variables;
3. canonical facility occupancy coupled bidirectionally to transport occupancy;
4. external terminals in the same commodity-network routing model;
5. exact parallel dimension cases with proof-derived bound sharing;
6. possible-graph connectivity propagation;
7. event-driven unique-support and local-continuation propagation;
8. guarded positive-item intersection propagation;
9. exact placement, rotation, port, endpoint, and residual-tuple partitions;
10. exact sparse legal external boundary-key domains, removing 4,800 impossible root values;
11. root-effective exact external side specialization with complete proof aggregation.

No item in this list preselects a placement, port, corridor, or route heuristically.

## Independent Review

Three independent reviews examined proof soundness, experimental isolation, and next-strategy
selection. The review loop found and fixed four blockers before accepting the slice:

1. a positive unary table was initially mistaken for a root-effective side restriction;
2. the selected supply count did not initially prove facility ownership;
3. child fixation checks initially covered ports but not all facility placements/rotations;
4. the selected build certificate initially recorded the requested side without proving that the
   actual declared/table/routing domains were exactly that side.

The post-run controlled-model audit then exposed one reporting omission: each selected key also
contributes one terminal-option crossing guard. The normalization and its construction provenance
were independently reviewed and passed. All final reviewers returned `PASS`.

## Verification

```text
cargo fmt --all
cargo check --workspace
cargo test --workspace
cargo build --release -p aic-cli --bin aic-prior-terminal-pair
```

The main CLI tests, dedicated prior-terminal CLI test, and data-library tests pass. The final release
run produced no diagnostic, invalid witness, evidence conflict, or blocked interpretation.

## Artifact

The final release artifact is `/tmp/aic-phase3-boundary-side-final.5G8DKx`. It contains:

- `summary.json`, `stdout.json`, and self-contained `summary.html`;
- north/east/south/west authoritative layout HTML;
- north/east/south/west root-observation layout HTML.
