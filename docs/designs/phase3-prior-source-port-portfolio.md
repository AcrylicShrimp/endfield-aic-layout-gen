# Phase 3 Prior-Source Port Portfolio

## Question

Do the eight residual Phase 3 leaves remain difficult after the older facility that supplies the
shared `item-xiranite-powder` network also has its output port fixed?

This is a closing control for facility endpoint choices. It does not assume that the source port is
the cause of the cliff and does not change production solver behavior.

## Predecessor Evidence

The target-facility completion portfolio partitions the selected diagnostic state into:

- 17 pair parents proven infeasible;
- 32 target-completion children proven infeasible; and
- 8 target-completion children still unknown.

Every residual child has the introduced facility coordinate, rotation, and ports fixed, and all four
ports of the preceding target facility fixed. The older same-item source terminal remains free.

## Exact Expansion

For each selected powder demand terminal, the diagnostic derives the same logical lane's stable
`:supply` terminal ID. It then selects the unique derived counterpart that is an output terminal
attached to a facility in the shared preceding reference. Zero or multiple facility-backed matches
are invalid diagnostic input. The compatible-port domain comes from the cumulative exact model.

For every target-completion child not proven infeasible, execute every source-port value exactly
once. The current data has eight retained parents and five compatible output ports, so the expected
matrix is 40 leaves.

The resulting exact coverage is:

```text
17 closed pair regions
union
32 closed target-completion regions
union
40 source-port leaves under the 8 unresolved regions
```

Only regions with a Pumpkin infeasibility proof are closed. A validated feasible parent remains
evidence of a witness and is still expanded so the source-port partition stays complete.

## Fixed And Free Decisions

Each source leaf fixes only:

- exact used dimensions `16 x 16`;
- the selected introduced-facility coordinate and rotation;
- all introduced-facility ports;
- the two selected preceding target demand ports;
- the remaining preceding target pipe and output ports;
- the old same-item source terminal's selected output port; and
- preceding facility placements from the exact shared reference.

Every other preceding terminal, route cell, route arc, item value, flow value, topology decision,
capacity decision, occupancy decision, logistics component, and bridge remains a solver decision.
No placement, port, corridor, path, or search-order heuristic is introduced.

## Outputs

The report must record:

- the complete target-facility completion stage and its exact preceding reference;
- source terminal ID, facility instance, reference port, and complete compatible-port domain;
- parent-to-child coverage and every source-port leaf;
- outcome, construction/search/first-incumbent timing, model scale, and native search counters per
  leaf;
- child-only outcome counts plus aggregate witness and infeasibility proof flags;
- separate preparation, source-wave, and total wall times; and
- machine-readable JSON, an HTML summary, and one standalone HTML result per leaf.

## Interpretation Boundary

- A validated leaf proves feasibility only for this restricted selected state.
- A proven-infeasible leaf closes only that exact source-port region.
- A timeout remains unknown.
- If residual leaves remain, all facility endpoints of the selected raw `item-xiranite-powder`
  network are fixed in those leaves. Other Phase 3 facility terminals remain solver decisions and
  must be reported before deciding whether to continue endpoint controls or inspect routing, flow,
  topology, and capacity.
- This diagnostic cannot prove global Phase 3 infeasibility or objective optimality.

## Stopping Point

Commit the complete matrix and its reviewed report before adding a new routing or flow propagator.
Use the residual result to choose one read-only exact route/flow/topology diagnostic.
