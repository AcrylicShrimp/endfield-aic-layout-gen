# Heavy Xiranite Phase 3 Endpoint Source-Only Control

## Result

The source-only control proves that the rejected internal-pipe source direction does not need any
demand-continuation split to become infeasible.

The selected network is `network:pipe:item-liquid-xiranite-poly`. Its fixed source at cell 48 has
two root-live outgoing continuations. The control partitions only those two source alternatives and
leaves all three demand continuations at cell 113 unrestricted:

| Case | Selected source arc | Earlier source arcs fixed to zero | Demand continuation | Authoritative | Observation | Root infeasible | Search ms | Decisions | Conflicts | Propagations |
| ---: | --- | ---: | --- | --- | --- | --- | ---: | ---: | ---: | ---: |
| 0 | `48 -> 32` | 0 | unrestricted | Proven infeasible | Proven infeasible | yes | 106 | 0 | 1 | 245,177 |
| 1 | `48 -> 64` | 1 | unrestricted | Unknown | Unknown | no | 5,006 | 54,758 | 3,868 | 5,550,711 |

The two canonical source cases are non-empty, pairwise disjoint, and exhaustive. No case finds an
incumbent, invalid witness, or evidence conflict. Every interpretation gate passes.

The result strengthens the preceding six-case endpoint report:

> Inside the inherited fixed key-24 leaf, selecting positive flow on `48 -> 32` is already
> contradictory before any search decision, regardless of which demand continuation is used.

Therefore the previous three demand children under source `48 -> 32` were not individually needed
to discover the proof. The source-side choice alone is sufficient. The complementary source
direction `48 -> 64` remains the live region and still contains the first-feasible cliff.

This remains a leaf-local implication, not a general production pruning rule. The run proves the
contradiction under the inherited fixed facility, port, boundary-terminal, and endpoint context. It
does not identify which semantic rule or combination of rules produces the root conflict.

## Exact Control Contract

The experiment reuses the complete accepted parent chain and changes only the endpoint-continuation
restriction:

- the source candidates remain the same canonical two-case partition;
- the selected source arc receives positive flow;
- every earlier source candidate is fixed to zero;
- later source candidates remain free;
- no demand arc is selected or excluded by the control;
- all facility placement, rotation, port, boundary-terminal, routing, item, flow, capacity,
  topology, bridge, and collision semantics remain unchanged.

The source candidates are:

| Case | Source cell | Selected arc | Canonical preceding arcs |
| ---: | ---: | --- | --- |
| 0 | 48 | `48 -> 32` | none |
| 1 | 48 | `48 -> 64` | `48 -> 32` |

The source has one positive flow unit and fixed terminal presence. Consequently every feasible
solution selects at least one outgoing positive-flow continuation. Selecting the first true
candidate canonically forms an exact two-case cover without requiring a unique route or excluding
branches, merges, cycles, or shared transport.

## Relationship to the Six-Case Parent

The parent endpoint portfolio crossed the two source cases with three demand cases. Its regions
and this control agree exactly:

| Source region | Parent demand children | Parent outcomes | Source-only outcome | Evidence compatible |
| --- | ---: | --- | --- | --- |
| `48 -> 32` | 3 | 3 proven infeasible | Proven infeasible | yes |
| `48 -> 64` | 3 | 3 unknown | Unknown | yes |

The report rejects either possible proof contradiction:

1. a validated parent child under a source region while the source-only region is proven
   infeasible; or
2. every parent demand child proven infeasible while the source-only region has a validated
   witness.

Neither contradiction occurs. `Unknown` is not interpreted as proof in either direction.

## Controlled Model Change

The selected key-24 parent has 63,385 variables, 161,632 constraints, 618,978 incidences, and
242,663 placement-routing incidences. Source-only cases retain every variable and every
placement-routing incidence. They add exactly one selected-positive constraint plus the canonical
number of earlier-zero constraints:

| Case | Variables | Constraints | Incidences | Placement-routing incidences | Expected unary additions |
| ---: | ---: | ---: | ---: | ---: | ---: |
| Parent | 63,385 | 161,632 | 618,978 | 242,663 | 0 |
| 0 | 63,385 | 161,633 | 618,979 | 242,663 | 1 |
| 1 | 63,385 | 161,634 | 618,980 | 242,663 | 2 |

The measured changes match the expected controlled axis. Authoritative and observation models are
identical within each case. Boundary certificates, continuation certificates, fixed-facility
contracts, fixed-port contracts, semantic model identities, and source restriction certificates
all pass.

For the root-infeasible case, the fixed-facility contract means the case was built from the same
certified inherited fixation. Dynamic fixed-domain observation does not apply because root
propagation rejects the model before a root snapshot can be captured.

## What Is and Is Not Proven

Proven:

- the two source-only cases exactly cover the inherited source choice;
- neither control case posts an explicit demand-continuation fixation, and all three demand
  continuations remain root-live in the surviving `48 -> 64` case;
- `48 -> 32` is impossible in the inherited fixed leaf regardless of demand continuation;
- every feasible solution in that leaf must use `48 -> 64` as its first positive source
  continuation;
- the source-only result and all six parent endpoint cases contain no logical evidence conflict.

Not proven:

- that the inherited key-24 leaf is feasible or infeasible;
- that the surviving `48 -> 64` region is feasible or infeasible;
- the minimal semantic cause of the `48 -> 32` root conflict;
- that the rejected direction can be removed outside this inherited fixed context;
- that source direction is the dominant cost inside the surviving region;
- that the fixed 16 by 16 controlled canvas is a production optimum or required game bound.

The 16 by 16 size is only the stable controlled fixture inherited from the earlier Phase 3
diagnosis. Production solving continues to enumerate exact dimension cases. A separate exact width
sensitivity experiment already showed that the same controlled leaf remains `Unknown` from 13 by
16 through 16 by 16, so merely removing three columns did not cross this cliff.

## Interpretation

The source-only control resolves the question posed by the preceding report. Demand selection was
not hiding three separate proofs. The native model can propagate the bad source direction to a
root conflict while the demand remains a three-way disjunction.

This removes the demand disjunction from the current root-conflict explanation. Special propagation
which merely combines those three demand proofs would add no additional root pruning for this one
`48 -> 32` leaf: the current formulation already rejects it without that split. The experiment does
not show that such propagation could not reduce proof cost or help another leaf.

The useful research target is now the surviving `48 -> 64` region. It still leaves global route,
item, flow, topology, capacity, bridge, external-terminal, and other-network decisions coupled and
performs 54,758 decisions and 5,550,711 solver propagations without a witness in five seconds.

## Next Exact Experiment

Continue directly inside the unresolved source-only child under source `48 -> 64`, while leaving
all three demand continuations unrestricted. Build an item-specific directed separator cut between
the fixed source and demand for `item-liquid-xiranite-poly`. In the controlled 16 by 16 leaf, a
horizontal separator between source row 4 and the next row separates source cell 64 from demand
cell 113.

For each directed cut arc `e`, define the exact material-carrying predicate:

```text
q_e <-> (flow(e) > 0 AND item(e) == item-liquid-xiranite-poly)
```

Then partition canonically by the first true south-directed `q_e`. This partitions a mandatory
interior cut while leaving the complete route on both sides, all three demand continuations,
branches, merges, cycles, northward recrossings, shared cells, bridges, and every other network
decision free. A false `q_e` must still allow the physical pipe arc to carry another item. The
design must fail closed unless the cut separates the fixed source and demand, positive net flow
requires at least one south crossing, and every legal directed crossing arc is represented.

Run one unpartitioned control containing the same biconditional `q_e` channels alongside the
canonical children. This distinguishes a gain from the redundant exact channeling itself from a
gain caused by the external cut partition. All child-versus-control model deltas must be audited.

Root-conflict provenance for `48 -> 32` remains useful as a separate instrumentation task. It
should record the emptied semantic domain and detecting constraint or propagator family if Pumpkin
exposes that information. Provenance is required before turning the observed leaf-local exclusion
into any general propagator rule.

## Independent Review

Three independent post-run reviews examined proof soundness, experiment evidence, and the next
exact strategy. All passed after two report-only wording corrections.

The reviews independently confirmed:

- the canonical two-case source cover is non-empty, disjoint, and exhaustive;
- the control posts no demand restriction and the surviving case retains all three demand arcs at
  root;
- the root-infeasible result has zero decisions and agrees between authoritative and observation
  solves;
- every child-versus-parent model delta matches only the source restriction;
- the source-region aggregation is compatible with all six preceding endpoint cases;
- the result is leaf-local and does not identify a general propagation rule or conflict cause;
- the next separator experiment should start from the source-only `48 -> 64` child, keep demand
  unrestricted, and include an unpartitioned control with identical material-crossing channels.

The experiment reviewer initially blocked two overbroad statements. The final report now
distinguishes the absence of an explicit demand fixation from dynamically observed root domains,
and claims only that combining the three demand proofs adds no further root pruning in this one
leaf. It does not exclude proof-cost improvements or usefulness in another leaf. No unresolved
soundness, measurement, or strategy blocker remains.

## Improvements Preserved in the Exact Baseline

The run retains shared belt and pipe layers, factored placement and port state, canonical physical
occupancy, external terminals inside commodity routing, exact dimension portfolios, possible-graph
connectivity, event-driven local continuation, guarded positive-item intersection, and every exact
placement, port, endpoint, boundary, and residual-tuple partition accumulated by the preceding
research. No layout, route, corridor, path-order, or other heuristic is introduced.

## Artifacts

- machine-readable report: `/tmp/aic-phase3-source-only.sm19xc/summary.json`;
- self-contained report: `/tmp/aic-phase3-source-only.sm19xc/summary.html`;
- per-case authoritative and observation wireframes:
  `/tmp/aic-phase3-source-only.sm19xc/case-*.html`.

The complete chained diagnostic took 330,904 ms. The new authoritative wave took 5,592 ms, the
observation wave took 5,603 ms, and the source-only experiment took 11,195 ms. The total includes
reconstruction of the complete accepted parent chain and is not the cost of only the two new cases.

## Verification

```text
cargo fmt --all
cargo test --workspace
cargo build --release -p aic-cli --bin aic-prior-terminal-pair
git diff --check
```

The final workspace run passes 34 main CLI tests, 2 prior-terminal CLI tests, and 294 data-library
tests.
