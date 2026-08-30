# Heavy Xiranite Phase-Zero Route-State Cliff Report

## Question

Which decision group first causes the clean 12 by 12 phase-zero output-belt plus input-pipe
problem to cross the five-second first-incumbent cliff?

This experiment uses release-mode Pumpkin 0.5. Every comparable case receives 5,000 ms. All
fixations are explicitly marked diagnostic-only and are not available on the production solve path.

## Corrected Network Subset

The earlier temporary network filter retained endpoint variables belonging to excluded networks.
This experiment reconstructs the selected model from its logical edges, so excluded networks also
lose their lanes, endpoints, terminals, and port candidates.

| Clean model | Route requirements | Endpoint variables | Route-arc variables | Total variables | Constraints | Incidences |
|---|---:|---:|---:|---:|---:|---:|
| Output belt only | 1 | 1,120 | 528 | 10,033 | 36,461 | 133,559 |
| Input pipe only | 1 | 224 | 528 | 9,137 | 31,069 | 108,471 |
| Output belt + input pipe | 2 | 1,344 | 1,056 | 17,321 | 59,654 | 200,263 |

The old pair measurement retained 3,584 endpoint variables. Its timing remains historical evidence,
but its endpoint and total-model counts are not a clean subset baseline.

## Results

| Case | Free decisions | First incumbent | Final result | Search | Objective `(area, tiles, turns, side, components)` |
|---|---|---:|---|---:|---|
| Pair free | Placement, ports, both routes | none | unknown | 5,000 ms | none |
| Output belt only | Placement, ports, belt route | 2,490 ms | rejected incumbent | 5,002 ms | none |
| Input pipe only | Placement, ports, pipe route | 1,773 ms | optimal | 2,266 ms | `(30, 1, 0, 6, 0)` |
| Pair, placement fixed | Ports, both routes | none | unknown | 5,000 ms | none |
| Pair, placement and terminals fixed | Both routes | none | unknown | 5,000 ms | none |
| Pair, output-belt route fixed | Input-pipe route | 1,072 ms | optimal | 1,442 ms | `(36, 2, 0, 6, 0)` |
| Pair, input-pipe route fixed | Output-belt route | 1,904 ms | optimal | 2,210 ms | `(36, 2, 0, 6, 0)` |
| Pair, input-pipe arcs zero | Output-belt route and all logical choices | 1,088 ms | optimal | 1,802 ms | `(36, 2, 0, 6, 0)` |
| Pair, output-belt arcs zero | Input-pipe route and all logical choices | none | unknown | 5,000 ms | none |
| Pair, both networks' arcs zero | Placement and ports | 73 ms | optimal | 113 ms | `(36, 2, 0, 6, 0)` |

Reference self-checks fixed placement, terminals, or both while keeping both networks' arcs zero.
They all proved the same optimum in 36-39 ms. These checks caught and then verified the repair of an
ambiguous reference-port mapper: port ID plus connection cell did not uniquely identify the owning
placement candidate. Endpoint options now retain their placement candidate, and warm starts use the
same unambiguous mapping.

## Located Cliff

The cliff is not placement or port selection for this pair:

- fixing placement does not produce a first incumbent;
- fixing placement and every terminal does not produce a first incumbent;
- fixing either complete network route makes the remaining exact pair model optimal within 2.3 s;
- fixing both networks' grid arcs to zero makes the complete placement-and-port optimization optimal
  in 113 ms.

The first located cliff is therefore **two simultaneously free route-state spaces**. The asymmetry
between the one-network zero-arc cases shows a secondary coupling: leaving the output-belt terminal
and placement choice free while the pipe route remains free is still hard, whereas leaving the pipe
terminal free with the belt route is tractable. This does not change the primary boundary because
fixing logical choices alone leaves both free routes intractable.

## Real Circulation Pollution

The clean unrestricted output-belt-only model found an incumbent at 2,490 ms, but independent
validation rejected it because active cells were outside every supply-to-demand path. Unlike the
earlier off-origin fixed-placement artifact, this case has no fixed coordinates. The incumbent
contains large positive-flow circulation disconnected from the co-located facility-output and
external-demand terminal pair.

This meets the previously stated revisit condition: circulation is now observed in an unrestricted
exact search and can consume the whole budget even though a valid zero-arc route exists.

## Exact Reduction Candidate

Both selected phase-zero networks contain one external/facility terminal pair whose supply and
demand:

- select the same port option variable;
- occupy the same connection cell;
- carry equal and opposite flow.

Their net terminal balance is therefore zero at every cell for every legal placement and port
choice. Setting every grid arc in such a network to zero preserves feasibility. Removing any
positive circulation cannot enlarge used geometry or increase any later objective and strictly
improves physical transport tiles whenever the circulation uses an additional tile.

This supports a semantics- and objective-preserving dominance rule:

> If every terminal contribution in a commodity network cancels at the same selected cell, fix all
> grid arcs and grid-arc flow variables in that network to zero before search.

This is not a general external-network shortcut and not a routing heuristic. It applies only after
the model proves cell-wise terminal cancellation symbolically. Networks with terminals at different
cells remain fully routed by the joint solver.

## Next Interactive Decision

The first actionable exact reduction is now identified. The next implementation slice can add the
proved cell-wise cancellation rule to the production formulation, then rerun the full three-network
phase-zero baseline. If the next cliff remains, route topology booleans, positive flow integers,
route-cell/arm auxiliaries, and cross-network objective coupling should be separated in that order.

