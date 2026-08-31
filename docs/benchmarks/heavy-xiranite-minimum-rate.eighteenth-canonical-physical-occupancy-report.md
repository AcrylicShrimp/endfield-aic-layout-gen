# Heavy Xiranite Canonical Physical Occupancy Reformulation Report

## Outcome

The exact solver now represents facility occupancy as canonical physical cell state and channels
placement, belt occupancy, and pipe occupancy through that state. A controlled exhaustive comparison
found no legal-state difference from the previous candidate-collision encoding. The reformulation is
therefore accepted as a semantics-preserving exact replacement, not a heuristic reduction.

It fixes a real propagation gap. If a facility is restricted to one footprint but several rotations
still represent that same footprint, every covered belt and pipe cell now becomes false at the root.
The previous encoding left all 25 belt and all 25 pipe cells free until one candidate literal became
true. Collision terms in the controlled 12 by 12 probe fell from 13,088 to 576.

The full Heavy Xiranite cumulative solve also improved structurally and found better five-second
incumbents in phases 1 and 2. However, it did not remove the global cliff: 13 by 13 phase 1 and 12 by
12 phase 3 still produce no first incumbent in 5,000 ms. Physical occupancy was a worthwhile exact
reformulation target, but it is not the dominant remaining blocker.

## Exact Reformulation

The previous encoding connected every placement-candidate Boolean directly to every belt and pipe
cell covered by that candidate. It could propagate a forced transport cell back to all conflicting
candidates, but it did not have one canonical variable for the statement “this physical cell is
occupied by some facility.”

The new encoding uses:

```text
placement choice
  -> per-instance footprint mask
  -> per-instance physical occupancy
  -> canonical facility_occupied[x,y]

internal route cells + same-layer external connector cells
  -> belt_occupied[x,y] / pipe_occupied[x,y]

facility_occupied[x,y] + belt_occupied[x,y] <= 1
facility_occupied[x,y] + pipe_occupied[x,y] <= 1
```

The canonical facility Boolean also enforces facility-to-facility non-overlap. Belt and pipe do not
exclude each other because they are separate physical layers. Placement, rotation, port selection,
external connector selection, internal network topology, and all route cells remain solver decisions.
No legal coordinate, port, connector, or routing state was removed.

The placement integer index is now authoritative. Candidate-selection Booleans are exact equality
literals for that index, rather than an exactly-one Boolean family plus a wide weighted equality.
Used geometry and the transport-tile objective consume the canonical occupancy variables.

## Controlled Equivalence And Root Propagation

The diagnostic uses one 5 by 5 Xiranite Oven in a 12 by 12 request ceiling. There are 256 legal
placement-and-rotation candidates. Cell `(6,6)` is covered by 100 of them.

Every combination of the 256 placements and four forced target-cell belt/pipe assignments was
enumerated for both encodings: 2,048 states per encoding. The accepted/rejected state sets were
identical, with zero mismatches.

| Metric | Candidate collision | Canonical occupancy |
| --- | ---: | ---: |
| Legal-state mismatches | 0 | 0 |
| Collision rows | 288 | 288 |
| Collision terms | 13,088 | 576 |
| Forced belt cell removes covering placements | 100 | 100 |
| Forced pipe cell removes covering placements | 100 | 100 |
| Exact placement forbids belt cells in footprint | 25 | 25 |
| Exact placement forbids pipe cells in footprint | 25 | 25 |
| Same-footprint coordinate restriction forbids belt cells | 0 | 25 |
| Same-footprint coordinate restriction forbids pipe cells | 0 | 25 |

The same-footprint row is the important difference. Four rotations can remain legal while all four
occupy the same 5 by 5 cells. The old collision rows require one candidate Boolean to become true
before transport is pruned. The canonical occupancy element sees that every supported candidate
occupies each footprint cell and fixes the shared physical state immediately.

The stronger controlled propagation costs more root work in isolation, approximately 0.84 ms versus
0.07 ms in the recorded run, because it performs element propagation. The full model results below
show that this extra local work is repaid by fewer and smaller downstream constraints.

## Cumulative 12 By 12 Result

Every row is the same Heavy Xiranite minimum-rate cumulative SCC problem, optimized in release mode
with 5,000 ms of search per phase. Objective vectors are `(used area, physical transport tiles,
route turns)`. Phase 0 is proven optimal; phase 1 and phase 2 incumbents are valid but unproven.

| Phase | Graph | v2 first | Canonical first | v2 result | Canonical result |
| ---: | --- | ---: | ---: | --- | --- |
| 0 | 1 facility, 4 requirements | 83 ms | 16 ms | optimal `(42, 4, 0)` | optimal `(42, 4, 0)` |
| 1 | 2 facilities, 7 requirements | 1,230 ms | 2,000 ms | feasible `(84, 39, 10)` | feasible `(77, 13, 2)` |
| 2 | 3 facilities, 10 requirements | 4,196 ms | 3,748 ms | feasible `(144, 112, 47)` | feasible `(132, 90, 34)` |
| 3 | 4 facilities, 13 requirements | none | none | unknown | unknown |

Phase 1 finds its first incumbent later, so the reformulation is not a uniform first-solution speedup.
It nevertheless finds nine incumbents instead of six and ends with a much smaller layout. Phase 2
finds its first incumbent 448 ms earlier, finds two incumbents instead of one, and also ends with a
better objective. All complete canonical witnesses passed independent validation.

## Model Structure

The canonical state adds occupancy variables, so raw variable count increases. It removes the much
wider repeated candidate-to-cell collision relations, reducing both constraints and terms.

| Phase | Variables v2 -> canonical | Constraints v2 -> canonical | Terms v2 -> canonical | Build v2 -> canonical |
| ---: | ---: | ---: | ---: | ---: |
| 0 | 15,770 -> 16,346 | 70,522 -> 64,408 | 201,137 -> 162,945 | 74 -> 66 ms |
| 1 | 32,622 -> 33,342 | 134,604 -> 121,656 | 414,263 -> 322,919 | 162 -> 142 ms |
| 2 | 48,573 -> 49,437 | 194,660 -> 174,878 | 630,619 -> 473,323 | 260 -> 218 ms |
| 3 | 56,452 -> 57,460 | 231,793 -> 205,465 | 764,169 -> 554,441 | 311 -> 255 ms |

At phase 3, constraints fall by 11.4%, terms by 27.4%, and construction time by 18.0%. Peak process
RSS falls from 274.70 MiB to 267.50 MiB. These are single-process RSS observations, while structural
counts are deterministic for the revision.

The 13 by 13 phase-1 sensitivity case tells the same structural story without a search success:

| Metric | v2 | Canonical |
| --- | ---: | ---: |
| First incumbent in 5,000 ms | none | none |
| Variables | 41,777 | 42,622 |
| Constraints | 175,024 | 158,651 |
| Terms | 535,657 | 420,116 |
| Build time | 210 ms | 182 ms |
| Peak RSS | 171.05 MiB | 164.47 MiB |

The reformulation reduces representation cost, but the extra row and column still expand another
coupled decision family enough to prevent a first incumbent.

## Improvements Retained In The Current Baseline

The canonical v3 formulation contains the accepted exact improvements accumulated so far:

1. Structured construction/search diagnostics, model-family counts, release-mode timing, isolated
   RSS, machine-readable JSON, and self-contained HTML for both success and failure.
2. Removal of the explicit cycle-free proof while retaining positive flow, terminal balance,
   conservation, capacity, topology, and collision semantics.
3. Exact cancellation of co-located equal-flow source/sink terminals.
4. One shared belt layer and one shared pipe layer instead of one dense grid per logical network.
5. Independent placement and port decisions instead of a flattened placement-times-port endpoint
   domain.
6. Straight external boundary connectors with three solver-selected legal side templates.
7. Port-selected variable-element geometry instead of a placement-times-port Cartesian scalar.
8. Cumulative SCC growth with a complete enlarged exact problem and non-binding hints from prior
   complete solutions.
9. Witness validation aligned with circulation, shared trunks, branches, and bridge semantics.
10. Canonical physical facility, belt, and pipe occupancy with bidirectional exact channeling.

None of these improvements preselects a placement, rotation, port, connector, or route.

## Remaining Cliff

The phase-3 canonical model contains 37,247 external-connector variables, 152,830
external-connector constraints, and 385,616 external-connector terms. This is 64.8% of all
variables, 74.4% of all constraints, and 69.6% of all terms. These counts are exactly unchanged by
the physical occupancy reformulation.

This does not prove that raw external-connector count alone causes the timeout. It does establish
the next narrow target: the interaction among selected facility placement, selected port,
solver-selected boundary template, used bounding box, and every cell along the resulting connector
ray. Physical collision is no longer the strongest unexplained coupling at the phase-3 cliff.

## Recommended Next Experiment

Decompose the external-connector geometry and used-bound coupling without changing its legal state
set. The controlled experiment should answer:

1. Which external connector decision first causes the 13 by 13 phase-1 incumbent cliff: port,
   template, exit side, used bound, or ray-cell activation?
2. When a boundary template or used bound is removed, how many connector cells and placement/port
   alternatives are pruned at the root?
3. Are repeated implication rows encoding a relation that can instead be expressed by exact
   variable-element or cumulative ray occupancy state?
4. Can one canonical connector geometry state replace repeated placement-port-template-to-cell
   clauses while preserving every legal connector and the exact objective?

Start with the existing 12 by 12 versus 13 by 13 phase-1 boundary because it isolates the cliff in
the same logical graph. Only after a controlled exact equivalence test should a candidate
reformulation be applied to cumulative phases 2 and 3.

## Verification

- `cargo fmt --all`
- `cargo check --workspace`
- `cargo test --workspace`: 178 tests passed
- exhaustive controlled equivalence: 2,048 states per encoding, zero mismatches
- optimized release-mode 12 by 12 cumulative phases 0 through 3
- optimized release-mode 13 by 13 phase 1
- independent witness validation for every complete layout
- self-contained HTML for every full-model result, including unknown results
- `/usr/bin/time -l` peak RSS for each isolated comparison process

## Artifacts

- Controlled baseline and canonical propagation probes:
  `docs/benchmarks/heavy-xiranite-physical-occupancy-propagation/`
- Full canonical cumulative JSON and HTML:
  `docs/benchmarks/heavy-xiranite-canonical-occupancy-cumulative/`
- Machine-readable normalized comparison:
  `docs/benchmarks/heavy-xiranite-canonical-occupancy-cumulative/comparison.json`
- Previous v2 cumulative baseline:
  `docs/benchmarks/heavy-xiranite-v2-cumulative-scc-growth/`
- Previous 13 by 13 baseline:
  `docs/benchmarks/heavy-xiranite-v2-phase3-bound-sensitivity/13x13.json`

## Decision Boundary

The canonical occupancy reformulation is implemented, verified, measured, and accepted as the new
exact baseline. It improves propagation and incumbent quality but does not remove the next search
cliff. No external-connector reformulation or heuristic restriction has been applied. Review this
checkpoint before beginning the next exact decomposition.
