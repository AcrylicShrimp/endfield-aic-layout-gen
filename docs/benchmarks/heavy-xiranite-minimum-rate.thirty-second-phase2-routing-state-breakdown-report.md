# Heavy Xiranite Minimum Rate: Phase 2 Routing-State Breakdown

## Result

The current five-second Phase 2 first-witness cliff is not attributable to one large downstream
routing family such as integer flow, item identity, or logistics-component selection. The smallest
observed trigger is one anonymous physical pipe-cell occupancy Boolean immediately downstream of a
fixed facility demand terminal.

With facility placement and all facility/external terminals fixed to a validated `12x12` reference:

- the unchanged routing problem remained `unknown` after 5,002 ms;
- fixing only pipe cell `(5,4)` to occupied produced a validated witness in 3,542 ms;
- fixing only pipe cell `(5,5)` to occupied produced a validated witness in 4,851 ms;
- fixing any one of the other seven cells on the same reference water route remained `unknown` at
  five seconds.

Cells `(5,4)` and `(5,5)` are the first two internal cells between the fixed water demand terminal
at `(5,3)` and the longer corridor to the fixed external supply at `(0,6)`. This isolates the
remaining cliff to the local-to-global routing-support choice at a terminal: Pumpkin must choose a
compatible terminal approach, directed arc, positive flow, item identity, and continuing support
before the existing local constraints propagate strongly.

This is a diagnostic result, not a production fixation or an approved routing heuristic.

## Scope and controls

- workload: Heavy Xiranite at minimum rate;
- cumulative production phase: 2;
- exact dimensions: `12x12`;
- facility anchor: `(0,1)`;
- port assignment partition: 5;
- placement and all terminal geometry: fixed to an independently validated reference;
- routing decisions: free except for the explicitly named diagnostic family or Boolean;
- per-case search budget: 5,000 ms;
- execution: optimized release binary, sequential cases;
- timeout classification: `unknown`, never infeasible.

The reference witness was reproduced in 14,940-15,103 ms depending on the run. Its objective was
area 144, 103 physical transport tiles, 40 turns, maximum side 12, and 24 logistics components.

## Improvements already established before this checkpoint

The current model and harness already incorporate these measured improvements from preceding
checkpoints:

1. one shared physical belt layer and one shared physical pipe layer replaced per-network dense
   routing grids;
2. facility placement and port selection became independent exact variables instead of one
   flattened placement-times-port candidate index;
3. facility and transport occupancy are channeled through shared physical occupancy constraints;
4. external connectors use the shared routing formulation instead of a separate shape generator;
5. exact used-dimension partitioning breaks the used-bounds/boundary-terminal cycle without losing
   legal solutions;
6. the Phase 2 placement, rotation, port, and terminal partitions can reproduce a validated exact
   routing reference;
7. physical transport-tile caps were measured and ruled out as a sufficient first-witness remedy.

Those changes reduced earlier model growth and moved the measured cliff. They did not eliminate the
Phase 2 routing-support cliff diagnosed here.

## A-E routing-state matrix

The letters have the following exact meanings:

- **A**: physical belt/pipe route-cell occupancy;
- **B**: item identity on each directional cell arm;
- **C**: directed grid-arc activation;
- **D**: directed grid-arc integer flow;
- **E**: splitter, converger, bridge, and bridge-rotation selections.

| Case | Added reference equalities | Outcome | Search ms | First witness ms | Observed tiles / turns |
| --- | ---: | --- | ---: | ---: | ---: |
| Routing-free baseline | 0 | unknown | 5,003 | - | - |
| A independently | 288 | validated feasible | 773 | 773 | 103 / 43 |
| B independently | 1,152 | validated feasible | 93 | 93 | 103 / 40 |
| C independently | 1,056 | validated feasible | 59 | 59 | 103 / 40 |
| D independently | 1,056 | validated feasible | 60 | 60 | 103 / 40 |
| E independently | 3,744 | validated feasible | 102 | 102 | 105 / 36 |
| A+B | 1,440 | validated feasible | 84 | 84 | 103 / 40 |
| A+B+C | 2,496 | validated feasible | 49 | 49 | 103 / 40 |
| A+B+C+D | 3,552 | validated feasible | 49 | 49 | 103 / 40 |
| A+B+C+D+E | 7,296 | validated feasible | 50 | 50 | 103 / 40 |

Every independent family can collapse the cliff because a complete reference assignment to any
one family indirectly fixes substantial routing structure. Therefore these timings do not show
that all five families are independent root causes. A is the smallest complete state family and
was subdivided further.

## A split by layer and Boolean value

| Fixed route-cell state | Equalities | Outcome | Search ms | Observed tiles |
| --- | ---: | --- | ---: | ---: |
| None | 0 | unknown | 5,002 | - |
| Belt and pipe, true and false | 288 | validated feasible | 790 | 103 |
| Belt only, true and false | 144 | validated feasible | 1,112 | 105 |
| Pipe only, true and false | 144 | validated feasible | 2,039 | 105 |
| Reference occupied cells only | 103 | validated feasible | 2,829 | 111 |
| Reference empty cells only | 185 | validated feasible | 928 | 98 |
| Belt occupied cells only | 49 | validated feasible | 4,886 | 109 |
| Belt empty cells only | 95 | validated feasible | 1,641 | 94 |
| Pipe occupied cells only | 54 | validated feasible | 2,264 | 106 |
| Pipe empty cells only | 90 | validated feasible | 1,610 | 102 |

Both positive support and exclusion of empty cells help. Empty-cell assignments are stronger
because they remove most alternatives. They are not necessary: positive occupied cells alone also
cross the cliff.

## Positive support split by commodity network

Only physical cells used by one reference commodity network were fixed to occupied. Item identity,
arc direction, flow, and component state remained free.

| Network | Occupied equalities | Outcome | Search ms |
| --- | ---: | --- | ---: |
| Belt / Carbon ENR | 2 | unknown | 5,002 |
| Belt / Xiranite ENR Powder | 31 | validated feasible | 4,139 |
| Belt / Xiranite Powder | 16 | unknown | 5,003 |
| Pipe / Liquid Sewage | 12 | validated feasible | 3,278 |
| Pipe / Water | 9 | validated feasible | 3,194 |
| Pipe / Liquid Xiranite | 2 | unknown | 5,002 |
| Pipe / Liquid Xiranite Lowpoly | 24 | validated feasible | 3,916 |
| Pipe / Liquid Xiranite Poly | 12 | validated feasible | 2,766 |

The result is not a monotone function of equality count. Twelve well-positioned cells can help
while sixteen cells on another network do not. Location and connection structure matter more than
raw route-cell count.

## Single-cell split

Water was the successful network with the fewest occupied equalities. Each of its nine reference
cells was then tested independently.

| Forced pipe cell | Structural position | Outcome | Search ms |
| --- | --- | --- | ---: |
| `(5,3)` | fixed facility demand terminal cell | unknown | 5,002 |
| `(5,4)` | first internal cell after facility demand | validated feasible | 3,542 |
| `(5,5)` | second internal cell after facility demand | validated feasible | 4,851 |
| `(0,6)` through `(5,6)` | external-supply corridor | unknown | 5,002-5,003 |

The successful equality is only `pipe_route_cell[x,y] = true`. It does not name water, choose an
arc direction, set a flow value, or fix the remaining route.

## Constraint-path explanation

The current formulation distributes one physical connection decision across several local state
families:

1. a route cell is the logical OR of its eight incoming/outgoing arm-presence Booleans;
2. each arm is the exact sum of a terminal-presence Boolean and incident selected grid arcs;
3. every selected directed arc is equivalent to a strictly positive bounded integer flow;
4. cell flow conservation balances terminal supply/demand and grid-arc flow;
5. arm item variables and the non-bridge cell item channel material identity;
6. branch and bridge constraints restrict legal multi-arm topology.

The fixed demand terminal creates a required incoming-flow disjunction, but it does not choose
which neighboring support continues toward a supply. With every support cell free, generic search
can spend the budget assigning low-level arcs, flow, items, and topology without completing one
consistent source-to-demand support. Fixing `(5,4)` or `(5,5)` activates at least one arm and hence
at least one positive-flow arc in the useful facility-side corridor; conservation and item/channel
constraints then propagate the rest strongly enough to obtain a witness.

## Final blocker for this iteration

The concrete blocker is **weak propagation and unhelpful generic branching across the
terminal-approach/support disjunction**. It is not:

- model construction time, which remains about 0.2 seconds per case;
- objective improvement after a first witness;
- facility placement, port assignment, or terminal coordinate choice in this controlled matrix;
- route cycle elimination;
- the physical transport-tile upper bound;
- one specific large A-E variable family by count alone.

This diagnosis is scoped to the controlled Phase 2 `12x12` routing-only boundary. It does not prove
that every later growth cliff has the same cause.

## Recommended next exact experiment

Test a first-class **terminal-approach state** as an exact reformulation. For each selected terminal
geometry, enumerate every locally legal incident-arm pattern and channel one finite-domain choice
directly to the corresponding route arms, selected arcs, flow requirements, item identity, and
component legality. All legal patterns must remain present; no corridor, preferred direction,
reference cell, or routing order may be imposed.

The comparison should measure root domain reduction, first-witness time, variables, constraints,
and the same objective vector against the unchanged baseline. A diagnostic alternative is an exact
portfolio over all legal terminal-approach choices, but adopting a routing-order brancher or any
preferred approach direction requires separate policy review because it may become a hand-written
routing heuristic.

Do not carry any reference equality from this report into the production solver.

## Artifacts

- `heavy-xiranite-phase2-routing-state-breakdown-5s/summary.json`
- `heavy-xiranite-phase2-routing-state-breakdown-5s/summary.html`
- `heavy-xiranite-phase2-route-cell-breakdown-5s/summary.json`
- `heavy-xiranite-phase2-route-cell-breakdown-5s/summary.html`
- one self-contained layout HTML for every successful and failed case in the two artifact folders.
