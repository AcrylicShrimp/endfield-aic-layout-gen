# Heavy Xiranite External Boundary Connector Cutover

## Scope

This checkpoint implements the accepted three-template external boundary connector contract in the
factored shared-layer exact formulation and makes the controlled cumulative SCC phase-zero command
use that formulation. External inputs and final outputs no longer instantiate free routing grids in
this experiment. Internal facility-to-facility requirements remain shared-layer routing networks.

For every external requirement, the solver jointly selects facility placement, facility rotation,
a compatible directional port, and exactly one of `forward`, `left`, or `right`. Connector cells
are derived through the final used bounding-box side and participate in collision, used geometry,
physical transport tile count, and turn count.

## Release-Mode Baseline

The Heavy Xiranite 12 by 12 cumulative SCC phase-zero case contains one Xiranite Oven and four
external logical requirements.

| Budget | Status | First incumbent | Search | Bounds | Area | Transport tiles | Turns | Internal networks | Validation |
| --- | --- | ---: | ---: | --- | ---: | ---: | ---: | ---: | --- |
| 5 s | feasible, unproven | 50 ms | 5,002 ms | 6 by 7 | 42 | 12 | 2 | 0 | passed |
| 30 s | proven optimal | 51 ms | 13,092 ms | 7 by 6 | 42 | 4 | 0 | 0 | passed |

The five-second witness is intentionally retained as evidence that first-feasible time is not
solution-quality time. The solver spent the full budget in the primary area stage, so the physical
transport-tile stage had not run. Its long connectors were valid but not tile-optimal.

The longer run proved the lexicographic vector `(42, 4, 0, 7, 0)`. Every external requirement uses
one transport tile, every selected template is `forward`, and no free routing network, grid arc,
flow variable, bridge, splitter, or converger exists in the phase-zero model.

## Model Evidence

The new formulation reports:

- 4 external connectors;
- 0 commodity networks and 0 network terminals;
- 0 route-cell, route-arc, network-flow, branch-component, bridge, or crossing variables;
- 15,336 external-connector variables;
- 257 placement variables and 12 endpoint variables; and
- 76 ms model construction in the five-second run.

The connector encoding is exact but not yet demonstrated efficient. It enumerates reachable
placement-port geometry keys against three template choices and links their derived cells to the
dynamic used bounds. The next checkpoint applies the repository cliff-diagnosis process to this
new formulation rather than treating the proven phase-zero result as an optimization conclusion.

## Cumulative Improvements

The current result follows a sequence of measured, semantics-preserving or explicitly approved
contract changes. The deltas below use the 12 by 12 cumulative phase-zero workload. They are not
presented as additive speedups because each checkpoint changed the baseline used by the next one.

| Checkpoint | Change | Measured effect | Search outcome |
| --- | --- | --- | --- |
| Release-mode execution | Run Pumpkin and the model in optimized Rust builds | Removed debug-build distortion from solve measurements | Established the release build as the only performance baseline |
| Circulation permitted | Removed the hard proof that every route must be acyclic | Removed 432 variables, 1,584 constraints, and 4,752 incidences | Complete phase zero still had no incumbent in 5 seconds |
| Shared transport layers | Replaced one dense physical grid per commodity with one belt layer and one pipe layer | Variables fell from 26,481 to 20,137 (-24.0%); constraints fell from 95,068 to 74,148 (-22.0%) | Complete phase zero still had no incumbent in 5 or 15 seconds |
| Factored endpoint selection | Replaced flattened placement-by-port choices with independent placement and port state | Endpoint states fell 66.9%; total incidences fell 22.7%; placement-routing incidences fell 43.2%; isolated peak RSS fell 10.2% | Complete phase zero still had no incumbent in 5 or 15 seconds |
| Deterministic external connectors | Replaced free external-terminal routing with solver-selected straight `forward`, `left`, or `right` boundary connectors | Internal networks, route grids, arc flow, branch, bridge, and crossing state all became zero in phase zero; total variables are 15,758 | First complete incumbent appeared in 50-51 ms; the four-tile optimum was proven in about 13.1 seconds |

The last change is the first checkpoint that crosses the phase-zero first-incumbent cliff. It does
not finish the performance work: four one-tile connectors still expand into 15,336 connector-family
variables and 70,510 total constraints. The next diagnosis must therefore distinguish the benefit
of the external-connection contract from the remaining cost of its current exact encoding.

The research tooling improved alongside the formulation. Reports now retain per-objective-stage
incumbents and bounds, construction and search time, variable-domain and factor-graph statistics,
independent witness validation, and self-contained HTML for both accepted and rejected outcomes.
The wireframe also exposes localized facility names, directional ports, transport occupancy, item
identity, rate, and solver status through click inspection.

## Artifacts

- `docs/benchmarks/heavy-xiranite-external-connectors-phase0/phase0.12x12.json`
- `docs/benchmarks/heavy-xiranite-external-connectors-phase0/phase0.12x12.html`
- `docs/benchmarks/heavy-xiranite-external-connectors-phase0/phase0.12x12.30s.json`
- `docs/benchmarks/heavy-xiranite-external-connectors-phase0/phase0.12x12.30s.html`
