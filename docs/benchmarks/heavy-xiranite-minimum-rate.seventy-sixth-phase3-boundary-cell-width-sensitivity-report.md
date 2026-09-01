# Heavy Xiranite Phase 3 Boundary-Cell Width Sensitivity

## Result

Reducing the exact grid from `16 x 16` to the minimum width that contains the controlled four
facilities does not cross the first-feasible cliff. Every fixed-width key-24 case remains `Unknown`
after five seconds:

| Size | Outcome | Build ms | Search ms | Decisions | Backtracks | Conflicts | Learned | Propagations |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `13 x 16` | Unknown | 374 | 5,005 | 62,295 | 6,168 | 6,167 | 6,167 | 6,594,060 |
| `14 x 16` | Unknown | 405 | 5,007 | 56,036 | 6,280 | 6,279 | 6,279 | 6,372,210 |
| `15 x 16` | Unknown | 467 | 5,006 | 56,662 | 5,348 | 5,347 | 5,347 | 5,979,748 |
| `16 x 16` | Unknown | 486 | 5,006 | 54,388 | 4,112 | 4,111 | 4,111 | 5,606,999 |

No case produced a first incumbent, an infeasibility result, an invalid witness, or conflicting
evidence. All interpretation gates passed.

The smaller grid substantially reduces model size but does not improve five-second search work.
Compared with `16 x 16`, `13 x 16` has 20.7% fewer variables, 19.9% fewer constraints, 20.0% fewer
incidences, 32.5% fewer unresolved route cells, and 34.6% fewer unresolved route arcs and flows.
Nevertheless it executes 17.6% more solver propagations and 50.0% more conflicts before the same
cutoff. Those operations may be cheaper in the smaller model, so this does not prove that width 13
is harder. Canvas width is a real model-size factor, but reducing it alone did not become the
current practical cliff breaker.

## Relationship to the Production Sweep

The production orchestration already enumerates exact dimension cases from sound lower bounds and
shares proven bounds between workers. Its Phase 3 failure is what led to the controlled
placement/port/endpoint diagnosis.

The previous `16 x 16` choice was not a production default, optimum, or game limit. It was the
known controlled canvas whose fixed placement requires height 16 and fits from width 13. This
experiment revisits all four fitting widths while preserving one exact endpoint leaf, so it tests
the user's canvas-size hypothesis without reintroducing the endpoint-location disjunction.

## Exact Experiment Contract

The accepted boundary-cell report is reproduced and its lowest unresolved key is decoded as the
semantic endpoint `(north, x=6, y=0)`. Each case rebuilds its own placement request and complete
`ModelInput` with the requested width and fixed height 16. The endpoint is re-encoded and audited
inside each rebuilt grid; its raw integer happens to remain 24 because it is on the top row.

Every width preserves:

- the same cumulative Phase 3 graph with four facilities, thirteen route requirements, and eight
  material networks;
- the same four facility placements and rotations;
- the same fifteen facility ports;
- the same selected external terminal and semantic endpoint;
- every route, item, flow, topology, capacity, collision, component, and other external-terminal
  decision as solver freedom;
- separate 5,000 ms authoritative and root-observation solves.

The width cases are neighboring exact fixed-size problems. They are not a partition of the
`16 x 16` case, and no feasibility or infeasibility conclusion transfers between widths.

## Exactness and Audit Gates

All gates passed:

| Assertion | Result |
| --- | --- |
| Boundary-cell parent is unblocked and unresolved | passed |
| Selected representative is the lowest unresolved singleton key | passed |
| Widths are positive, increasing, within the request ceiling, and include the parent | passed |
| Every inherited fixed facility footprint fits every width | passed |
| Facility, requirement, and material-network identities match across rebuilt inputs | passed |
| Production boundary-key generator is the only certificate oracle | passed |
| Selected build and root domains equal the width-specific singleton | passed |
| Every other external build domain equals the complete width-specific legal domain | passed |
| Authoritative and observation certificates agree | passed |
| Authoritative and observation formulation, exact model metrics, and complexity metrics match | passed |
| Width-independent semantic model counts match across cases | passed |
| Reported grid cells equal `width * 16` | passed |
| Four facility and fifteen facility-port fixation contracts pass | passed |
| No invalid witness or evidence conflict appears | passed |

The experiment initially failed independent review because its first draft changed only the
used-width equality while reusing a `16 x 16` grid. It also used a perimeter-only boundary-key
oracle that disagreed with the solver's variable-used-bounds domain. The final implementation
rebuilds `ModelInput` per width and directly reuses the production boundary-key generator.

## Model and Root-State Scaling

| Width | Grid cells | Variables | Constraints | Incidences | Root route cells | Root arcs | Root flows | Branch components | Bridge rotations | Arm items |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 13 | 208 | 50,275 | 129,526 | 494,874 | 199 | 664 | 664 | 1,652 | 676 | 5,677 |
| 14 | 224 | 54,645 | 140,228 | 536,242 | 231 | 768 | 768 | 1,908 | 764 | 6,656 |
| 15 | 240 | 59,015 | 150,930 | 577,610 | 263 | 892 | 892 | 2,164 | 932 | 7,809 |
| 16 | 256 | 63,385 | 161,632 | 618,978 | 295 | 1,016 | 1,016 | 2,420 | 1,060 | 8,998 |

All scale columns decrease monotonically with width. Other external root-domain cardinalities also
shrink from four domains of 53 and five of 54 at width 16 to four domains of 42 and five of 43 at
width 13. The controlled terminal remains singleton in every case.

This is a combined grid/domain-scale experiment. It cannot attribute the model reduction solely to
route state because placement candidates and other external domains also shrink when the exact
ceiling shrinks.

## Interpretation

The remaining cliff is not broken merely by removing three columns from this controlled fixture.
At width 13 the root has hundreds fewer route, flow, component, and item states, yet Pumpkin still
finds neither a witness nor a proof and performs more propagation and conflict work. The smaller
problem can be more constrained without yielding a first solution under the current generic search
order and five-second cutoff.

This result does not show that width has no effect on eventual solve time, nor that any width is
feasible. It shows only that exact width reduction from 16 to 13 is insufficient to break the
five-second first-feasible cliff for this controlled key-24 fixture.

The `13 x 16` and `15 x 16` root snapshots also expose an unregistered first-decision name. This is
an instrumentation gap, not proof of a semantic difference, and should not be used as a pruning
premise.

## Next Exact Experiment

Return to the fixed `16 x 16`, key-24 leaf and partition the mandatory first continuation at both
ends of the final-product belt network. Current evidence indicates two root-live outgoing arcs at
the singleton facility supply and two root-live incoming arcs at the singleton external demand,
forming a `2 x 2` portfolio of four source/demand continuation pairs.

Before constructing children, census the actual endpoint flow units and every root-live incident
arc. Fail closed unless positive terminal flow makes one continuation mandatory and the four cases
form a non-empty, pairwise-disjoint exact cover. Each child fixes only the selected positive source
and demand continuation; every interior route, item, flow, component, topology, and other network
decision remains free.

Independent reviewers also considered the complete state of the first observed pipe bridge cell
(`NoBridge` or one catalog-defined rotation). That is exact, but the native brancher already starts
on one of its rotation Booleans. The endpoint-continuation split is preferred because it partitions
a mandatory path cut rather than an optional component at an arbitrary cell. If the endpoint census
does not prove the expected four-way cover, do not force it; fall back to a canonical partition over
all live incident arcs.

## Improvements Preserved in the Exact Baseline

The current formulation retains the cumulative exact work:

1. shared belt and pipe physical layers;
2. factored placement and port variables;
3. canonical physical occupancy coupled bidirectionally to transport occupancy;
4. external terminals in the same commodity-network routing model;
5. exact parallel dimension cases and proof-derived bound sharing;
6. possible-graph connectivity propagation;
7. event-driven unique-support and local-continuation propagation;
8. guarded positive-item intersection propagation;
9. exact placement, rotation, port, endpoint, and residual-tuple partitions;
10. exact sparse legal external boundary-key domains;
11. exact external side and cell specialization;
12. semantic endpoint preservation across independently rebuilt width models.

## Independent Review

Three independent reviews examined formulation soundness, experimental isolation, artifacts, and
the next exact split. They found and closed three blockers before the release run:

1. changing only the used-width equality did not change the actual routing grid;
2. a duplicate perimeter-only helper disagreed with the production variable-used-bounds boundary
   domain;
3. same-width authoritative and observation models were initially compared with a lossy
   cross-width signature.

The final implementation rebuilds each complete input, shares the production boundary generator,
requires full same-width model equality, and separately audits cross-width logical identities. The
post-run reviewers agreed that width reduction did not break the observed cliff. They cautioned
that all cases are cutoff-censored `Unknown`, so the result is not a proof that grid scale is
irrelevant or that the smaller model is harder.

## Verification

```text
cargo fmt --all
cargo check --workspace
cargo test --workspace
cargo build --release -p aic-cli --bin aic-prior-terminal-pair
```

## Artifact

The final release artifact is `/tmp/aic-phase3-boundary-width.uXQbkt`. It contains:

- `summary.json`, `stdout.json`, and self-contained `summary.html`;
- authoritative and root-observation HTML for every width.

The four new width cases took 44,069 ms. Total wall time was 352,253 ms because the current CLI
reproduced the accepted parent chain first; the parent boundary-cell report ended at 308,184 ms.
