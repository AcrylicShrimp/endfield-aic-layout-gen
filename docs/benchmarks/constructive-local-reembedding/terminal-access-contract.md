# Relaxed terminal-access diagnosis

## Scope and inputs

This read-only diagnostic asks which occupied facility-terminal **cells** a path
must cross in a fixed finite grid after a proposed translation. It does not assign
ports, route a factory, validate a candidate, or change the production constructor.
Inputs are integer endpoints and inclusive bounds (at most 250,000 cells), a set
of hard obstacle keys, a map from soft terminal-cell keys to nonempty owner arrays,
and a positive expansion budget. The replay wrapper uses translated facility bodies
and same-layer external terminals as hard obstacles, and same-layer facility
terminals as soft obstacles. Ordinary wiring, component geometry, port arms and
other-layer terminals are deliberately ignored. Endpoint terminal cells are exempt;
bodies at endpoints are never exempt. The canvas is the retained witness extent,
not a production feasibility limit.

## Output and exactness

`minimumTerminalReleasePath` returns `status`, `expanded`, and a structured diagnostic
with code, severity, stage and endpoint entity. A `found` result additionally contains
`cells`, `steps`, `blocker_cells`, and `blockers` (position and every owner at that
position). Owners are copied to prevent mutation through the report. Owner identity
includes network index, terminal ID, instance, port, item and direction.

Within this relaxation, the objective is minimum distinct soft cells, then steps.
Each step costs one; entry into a soft non-endpoint cell adds N+1, where N is the
canvas cell count. Positive costs make an optimum simple, so steps are at most N-1
and each cell is visited once. The weight therefore gives the stated lexicographic
objective. A* uses Manhattan distance as a consistent lower bound. Equal costs use
stable grid IDs. Multiple owners at one cell count as one cell, NOT one reassignment.
All owners must be dealt with before any real path can claim to clear that cell.

`no-path` is exhaustive only for this finite relaxation. `work-budget` is unknown,
not disconnection. `blocked-endpoint` includes outside-canvas endpoints. Invalid
input throws `terminal-access-invalid-input`. Other codes are
`terminal-access-found`, `terminal-access-no-path`, `terminal-access-work-budget`,
and `terminal-access-blocked-endpoint`, at stage `terminal-access`, severity `info`.
No result is a global infeasibility or feasible-wiring claim. The chosen shortest
relaxed path is a proposal guide, not an exhaustive set of blocker alternatives.

## Replay and visual outputs

The explicit priority replay records `terminal_access` alongside body-only controls
for retained-port failures, with bounds and the declared relaxation. Native rendering
overlays a dashed relaxed path and obstruction squares on unchanged validated geometry.
It explicitly labels the overlay NOT a valid route. No relaxed route is passed to
reembedding or accepted as a factory witness. Existing fixed-geometry and full-factory
validators stay authoritative for any later coordinated assignment experiment.

## Verification gates

Compare the search against brute-force simple paths on every 3x3 hard/soft/free
assignment with fixed opposite endpoints. Cover budget exhaustion, same endpoints,
endpoint exemptions, hard/soft overlap, outside bounds, shared owners, determinism,
and input/output isolation. Replay the actual upper-bank failure and inspect its
native full and focused renders. Record findings before choosing a repair neighborhood.
