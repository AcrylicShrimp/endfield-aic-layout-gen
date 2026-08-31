# Heavy Xiranite Minimum Rate: Phase 2 Connectivity Witness

## Result

An explicit declarative parent/depth connectivity forest is exact, but it does not remove the
remaining Phase 2 first-witness cliff. It makes the controlled routing problem consistently about
twice as slow.

With `12x12` dimensions, all three facility placements, and all facility/external terminal
geometry fixed to the same validated reference:

- the unchanged formulation found a validated witness in 5,810 ms;
- the connectivity-witness formulation found a validated witness in 12,010 ms;
- a complete rerun measured 5,398 ms versus 11,316 ms;
- both cases remained `unknown` with no incumbent at a five-second budget.

The witness formulation therefore preserves the intended semantics but is rejected as a
performance strategy. It remains diagnostic-only and is disabled in every production solve path.

## Question and controls

The experiment asked whether the existing local material-flow constraints fail because they do not
give Pumpkin an explicit global proof that every selected demand can reach a compatible supply.

The comparison held constant:

- workload: Heavy Xiranite at minimum rate;
- cumulative production phase: 2;
- exact dimensions: `12x12`;
- all facility placements, rotations, ports, and external terminal cells;
- every original placement, item, flow, topology, capacity, and collision constraint;
- feasibility-only search and equal per-case wall-clock budgets.

Only the redundant connectivity proof was added. Physical route cells, directed arcs, item
assignments, integer flow, and logistics components remained solver decisions in both cases.

## Exact witness formulation

For each commodity and physical grid cell, the experiment added:

- one reached Boolean;
- one root Boolean derived from selected compatible supply terminals;
- one bounded integer depth;
- one possible proof-parent Boolean for every directed physical grid arc.

Every selected demand must be reached. Every reached non-root cell has exactly one selected parent,
and a parent must be a real selected positive-flow arc carrying the same commodity. Parent depth
increases by one, so the selected proof arcs form a forest rooted at compatible supplies.

The proof forest is only a subset of the real routing graph. The real graph may still contain
shared trunks, splits, convergences, multiple independent roots, and extra cycles. The formulation
does not restore fixed logical source-to-target pairs.

## Semantic check

A controlled external-route fixture was solved both with and without the witness. Both formulations
accepted and independently validated the same objective vector. This covers the representative
construction used by the experiment and guards against accidentally turning the proof forest into
a fixed path or single-source tree.

The general equivalence argument is:

1. every witness solution satisfies the unchanged original model because the witness only refers
   to selected real arcs and terminals;
2. every valid positive material flow contains at least one supply-to-demand path for each served
   demand;
3. selecting those paths and removing proof-only cycles yields a rooted parent forest without
   changing the physical route.

## Five-second result

| Case | Outcome | Build ms | Search ms | First witness | Variables | Constraints | Incidences |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Baseline | unknown | 200 | 5,003 | - | 23,972 | 82,648 | 285,128 |
| Parent/depth forest | unknown | 244 | 5,003 | - | 31,652 | 118,568 | 367,368 |

Neither timeout is an infeasibility result.

## Thirty-second result and rerun

| Run | Case | Outcome | Build ms | First witness ms | Observed tiles / turns |
| --- | --- | --- | ---: | ---: | ---: |
| Recorded | Baseline | validated feasible | 198 | 5,810 | 96 / 37 |
| Recorded | Parent/depth forest | validated feasible | 253 | 12,010 | 100 / 30 |
| Rerun | Baseline | validated feasible | 191 | 5,398 | 96 / 37 |
| Rerun | Parent/depth forest | validated feasible | 247 | 11,316 | 100 / 30 |

These are feasibility-only runs. The different tile and turn counts are incidental first
witnesses, not an optimization-quality comparison.

Across the two long runs, the witness took 2.07x and 2.10x as long to produce its first validated
solution. The repeated result makes ordinary timing noise an unlikely explanation.

## Added proof state

| Added family | Variables |
| --- | ---: |
| Reached | 1,152 |
| Supply root | 1,152 |
| Directed parent arc | 4,224 |
| Depth | 1,152 |
| **Total** | **7,680** |

The proof also added 35,920 constraints. Relative to the controlled baseline this is:

- 32.0% more variables;
- 43.5% more constraints;
- 28.8% more factor-graph incidences;
- 10.9% more placement-routing incidences.

## Why it became slower

The current model already expresses material connection through local positive flow, conservation,
item identity, and terminal balance. The new forest expresses a second path-selection problem over
the same grid using ordinary local Boolean and integer constraints.

It does not give Pumpkin a global view of the remaining possible graph. In particular, it does not
directly detect that:

- a demanded terminal has become disconnected from every compatible supply;
- a candidate terminal lies in an impossible connected component;
- one remaining cut arc is mandatory for a demanded component.

Those facts still emerge only after many local arc, reachability, parent, and depth assignments.
The generic search must now choose both the physical route and its redundant proof certificate.
The added certificate is therefore useful for validation, but weak as propagation.

## What this checkpoint rules out

This experiment rules out the following proposal as the next performance fix:

> Add a dense per-commodity parent/depth forest using ordinary declarative constraints and expect
> the redundant connectivity variables to guide the existing search.

It does not rule out connectivity reasoning itself. It shows that merely compiling connectivity
back into more local grid decisions is insufficient.

## Recommended next exact experiment

Evaluate a custom global connectivity propagator over the existing possible and mandatory route
graph. The propagator should add no path candidates and choose no preferred route. For each
commodity it should:

1. build connected components from arcs that are still possible;
2. fail immediately when a required demand component cannot reach any compatible possible supply;
3. remove terminal or placement choices whose cells cannot participate in such a connection;
4. force a sole remaining cut arc only when every legal connection must cross it;
5. explain each conflict or propagation using the excluded arcs and selected terminal facts needed
   by lazy-clause generation.

This is a separate implementation strategy, not an approved production change yet. It must retain
same-item fungibility, multiple supplies, disconnected valid supply-demand components, shared
trunks, and cyclic physical graphs. Start with the same controlled Phase 2 comparison and small
exhaustive equivalence fixtures.

## Improvements retained from earlier checkpoints

This negative result does not undo the exact improvements already present:

1. one shared physical belt layer and one shared physical pipe layer;
2. independent facility placement and port-selection variables;
3. shared facility/transport physical occupancy propagation;
4. external connectors represented by the common routing model;
5. exact used-dimension partitioning and parallel portfolio solving;
6. cumulative SCC growth with prior exact solutions used only as non-binding hints;
7. exact coordinate, rotation, port, terminal, route-cell, and tile-cap diagnostic partitions;
8. structured JSON and self-contained HTML for both successful and failed runs.

## Artifacts

- `heavy-xiranite-phase2-connectivity-witness-5s/summary.json`
- `heavy-xiranite-phase2-connectivity-witness-5s/summary.html`
- `heavy-xiranite-phase2-connectivity-witness-30s/summary.json`
- `heavy-xiranite-phase2-connectivity-witness-30s/summary.html`
- baseline and parent/depth-forest self-contained layout HTML for both recorded budgets.
