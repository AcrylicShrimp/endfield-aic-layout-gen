# Heavy Xiranite Shared Transport Layer Report 8

## Question

Can the exact joint placement-and-routing model replace one dense physical grid per commodity
network with one shared physical grid per transport layer, without preselecting placement, ports, or
routes, and does that change cross the phase-zero first-incumbent cliff?

This checkpoint implements and measures that alternative formulation. It does not replace the
production solver. The existing dense formulation remains the reference path.

## Exact Formulation Under Test

The experimental formulation creates one belt grid and one pipe grid. Each directed physical arm
stores whether it is active and which commodity item owns it. A selected directed arc joins equal
item identities and carries flow. Facility terminals constrain the adjacent arm to the terminal's
item. Splitters, convergers, crossings, collision rules, facility placement, rotation, directional
port choice, flow balance, capacity, used geometry, and the full lexicographic objective remain
inside the same Pumpkin model.

Consequently, the experiment does not construct a placement, choose a port, choose a route order,
crop a routing corridor, or score post-hoc candidates. It preserves the configured 12 by 12 legal
solution set and objective. Port endpoint variables intentionally remain unchanged because port
choice still depends on facility placement and rotation.

The important representational change is:

```text
dense reference: physical route state = grid cells times commodity networks
shared experiment: physical route state = grid cells times transport layers
                                     plus per-arm item identity
```

Multiple terminals for the same item can therefore use a solver-selected common trunk, splitter,
or converger. Different items cannot occupy the same non-crossing arm. Belt and pipe remain
separate layers.

## Correctness Check

A small two-belt-item instance was solved by both formulations to proven optimality. Both produced
the same objective vector and passed the existing independent layout-witness validation. This is a
focused equivalence check, not a general proof of formulation equivalence.

The production dense solver was not changed. The experimental API and CLI are research-only, and
reports identify the new formulation as `joint-shared-transport-layer-v1`.

## Controlled Heavy Xiranite Run

- Workload: `heavy-xiranite-minimum-rate`
- Cumulative SCC phase: 0
- Facilities: 1
- Commodity networks: 3, comprising 2 belt items and 1 pipe item
- Logical route requirements: 4
- Network terminals: 8
- Request ceiling: 12 by 12
- Solver: Pumpkin 0.5, release build
- Search budget: 5,000 ms independently for each formulation
- Solver search: unchanged Pumpkin search
- Heuristic reduction or fallback: none

The committed artifacts are:

- `heavy-xiranite-shared-layer-comparison/comparison.json`
- `heavy-xiranite-shared-layer-comparison/dense.html`
- `heavy-xiranite-shared-layer-comparison/shared-layer.html`

The command writes JSON and both HTML views for success, timeout, infeasibility, and invalid input:

```bash
target/release/aic-cli research compare-first-phase-shared-layer \
  --workload data/benchmarks/heavy-xiranite-minimum-rate.workload.json \
  --workspace-root . \
  --placement-request data/benchmarks/requests/placement.12x12.request.json \
  --time-limit-ms 5000 \
  --output-dir docs/benchmarks/heavy-xiranite-shared-layer-comparison
```

## Result

Both formulations exhausted their independent five-second search budgets without a first complete
incumbent. Both results are `unknown`, not infeasible. A supplementary 15-second release run also
returned `unknown` with zero incumbents for both formulations.

| Metric | Dense per-network grid | Shared per-layer grid | Change |
| --- | ---: | ---: | ---: |
| Variables | 26,481 | 20,137 | -6,344 (-24.0%) |
| Boolean variables | 24,554 | 17,778 | -6,776 (-27.6%) |
| Integer variables | 1,927 | 2,359 | +432 (+22.4%) |
| Log2 domain volume | 29,102.76 | 22,829.76 | -6,273.01 (-21.6%) |
| Constraints | 95,068 | 74,148 | -20,920 (-22.0%) |
| Constraint terms | 330,551 | 282,995 | -47,556 (-14.4%) |
| Factor-graph incidences | 323,383 | 279,411 | -43,972 (-13.6%) |
| Model construction | 122 ms | 144 ms | +22 ms |
| Search | 5,000 ms | 5,000 ms | same budget |
| First incumbent | none | none | unchanged |

Construction timing is a single-run observation. Structural counts are deterministic for this
input and formulation revision.

The integer-variable increase is expected: the shared model removes duplicated Boolean route
state but adds 1,440 small-domain arm-item variables. That item identity is what lets one physical
layer represent every commodity without preassigning routes.

## Variable-Family Breakdown

| Variable family | Dense | Shared | Delta |
| --- | ---: | ---: | ---: |
| Placement | 256 | 256 | 0 |
| Endpoint options | 3,584 | 3,584 | 0 |
| Route cells | 432 | 288 | -144 |
| Route arcs | 1,584 | 1,056 | -528 |
| Arc flow | 1,584 | 1,056 | -528 |
| Terminal presence | 3,456 | 2,304 | -1,152 |
| Directional route arms | 3,456 | 2,304 | -1,152 |
| Arm item identity | 0 | 1,440 | +1,440 |
| Branch components | 3,456 | 2,304 | -1,152 |
| Bridges | 288 | 288 | 0 |
| Bridge rotations | 1,152 | 1,152 | 0 |
| Per-network crossing owners | 864 | 0 | -864 |
| Objective auxiliaries | 6,369 | 4,105 | -2,264 |

The result confirms the intended scaling change for physical routing state. It does not make the
model independent of the number of items: each arm's item domain grows with the number of items on
that transport layer, and terminals still connect item identity to placement-dependent port
options.

## Remaining Constraint Cost

The largest shared-model families by terms are:

| Constraint family | Constraints | Terms | Role |
| --- | ---: | ---: | --- |
| Branch topology | 14,688 | 83,680 | splitter/converger selection and degree/flow consistency |
| Item assignment | 11,968 | 33,344 | active arms, arcs, terminals, crossings, and item identity |
| Turn definition | 10,912 | 27,520 | exact third objective |
| Terminal presence | 9,472 | 23,808 | placement-dependent endpoint to route-arm linkage |
| Used geometry | 6,858 | 20,282 | exact occupied bounding box and tile count |
| Bridge crossing | 6,752 | 18,160 | legal same-layer crossings |
| Line capacity | 3,292 | 13,628 | item-specific capacity on shared physical state |
| Transport collision | 288 | 13,088 | facility and layer occupancy exclusion |

The shared factor graph remains one connected component. Direct placement-routing constraints still
have 128,220 incidences, and objective variables still contribute 51,271 incidences. The model is
smaller, but not decomposed by the new representation.

## Interpretation

The proposed shared-state idea works as a model-size reduction: three commodity grids become two
physical layers and the exact model loses roughly one fifth to one quarter of its recorded search
state. This is meaningful and should remain available as an experimental architecture.

It does **not** cross the observed phase-zero cliff. That outcome is consistent with the input:
phase zero contains only three commodity networks, so replacing three grids with two has a limited
ceiling. The unchanged 3,584 placement-dependent endpoint choices and the new item-assignment
coupling still bind logical terminals to physical routing. Branch topology, turn accounting, and
used-geometry objectives also remain broad, layer-wide constraint systems.

The result neither disproves the shared-layer architecture nor justifies adopting it as the
production solver. It says that physical-grid duplication was a real cost, but was not the sole
cause of the first-incumbent failure.

## Next Interactive Decision

Stop here before another formulation change. The next research checkpoint should isolate the
remaining shared-model cliff rather than add an unmeasured optimization. The most useful exact
ablation order is:

1. satisfaction core versus objective auxiliaries in the shared formulation;
2. branch/splitter/converger topology;
3. item-assignment constraints and domains;
4. placement-dependent terminal linkage;
5. bridge/crossing state.

Each ablation must remain diagnostic-only if it changes semantics. The next adopted change, if any,
should be an exact reformulation whose legal solutions and objective quality are preserved.
