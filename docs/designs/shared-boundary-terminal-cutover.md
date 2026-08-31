# Shared Boundary Terminal Cutover

## Status

Accepted for implementation on 2026-08-31.

This contract replaces dedicated straight external connectors with solver-selected boundary
terminals inside the same exact commodity networks used by facility-to-facility transport.

## Motivation

An external input or output is physically a belt or pipe endpoint whose other side lies outside the
current blueprint. It is not a separate transport system. The previous formulation removed every
external edge from commodity-network routing, built one exclusive straight ray per requirement,
and merged only the resulting occupancy into the shared belt or pipe layer. That representation:

- duplicated grid state per external requirement;
- prohibited same-item trunk sharing between external and internal flow;
- imposed a hand-written straight-ray shape;
- modeled material flow and external physical occupancy in different subsystems.

The cutover removes that distinction.

## Input Contract

The input remains the complete cumulative `ModelInput`:

- facility instances and all legal rotations and origins;
- logical material edges, including edges with an `External` source or target;
- belt or pipe transport kind, item, rate, and capacity for every edge;
- caller-supplied maximum width and height as hard ceilings;
- optional non-binding hints from a previous complete exact solution.

External endpoints include both true blueprint inputs or outputs and temporary frontier endpoints
introduced by cumulative SCC growth. They use identical routing semantics.

## Exact Solver Contract

### One Shared Physical Layer Per Transport Kind

Every belt path, including paths that reach an external endpoint, uses the single shared belt
layer. Every pipe path uses the single shared pipe layer. No external requirement owns a private
cell mask or private route grid.

Facility occupancy excludes both transport layers. Belt and pipe remain different physical layers
and may occupy the same `(x, y)` coordinate.

### Commodity Networks

All logical edges are normalized by transport kind and item before routing. External terminals
participate in the same supply, demand, conservation, item-assignment, direction, topology, and
capacity constraints as facility terminals.

Logical edges do not require dedicated source-to-target paths. Compatible same-item flow may share
trunks, split, and converge. Different items may not occupy the same belt or pipe arm state.

### Boundary Terminal Decision

Each external endpoint selects one terminal `(cell, outward side)` from the complete perimeter of
the actual used bounding box:

- north: `y = 0`;
- west: `x = 0`;
- east: `x + 1 = used_width`;
- south: `y + 1 = used_height`.

The terminal cell is part of the shared route geometry and therefore part of the used bounding box.
Corner cells may expose either incident outward side. No side, coordinate, or route shape is chosen
outside the solver.

The terminal injects flow for an external input and removes flow for an external output. Its
outward side becomes the terminal arm direction. The route between the boundary terminal and any
facility port is otherwise ordinary shared-network routing.

### No Shape Heuristic

The solver does not require a straight path, a shortest-path template, a preferred side, a fixed
boundary coordinate, or an exclusive connector lane. Compactness is selected only by the accepted
lexicographic objective:

1. minimum used bounding-box area;
2. minimum physical belt and pipe tile count;
3. minimum total route turns;
4. later exact tie-breakers.

This cutover deliberately restores the full legal routing search space. Search-space growth is a
measured research result, not grounds for reintroducing a shape heuristic.

## Output Contract

External endpoints appear only as `External` terminals inside `transport_networks`. Each reports:

- external node ID;
- input or output direction;
- material and rate through its containing network and terminal;
- selected boundary position;
- selected outward boundary side.

The obsolete top-level `external_connectors` collection, connector templates, connector turns, and
connector-owned cells are removed in a hard schema cutover. HTML renders external terminals as
boundary source or sink markers attached to the shared route cells.

## Invariants

1. Every prepared external endpoint has exactly one selected boundary terminal.
2. Every selected external terminal lies on the matching side of the actual used bounding box.
3. External terminal flow participates in the same conservation and line-capacity equations as all
   other terminal flow.
4. A route cell used by external flow is indistinguishable from any other shared route cell for
   occupancy, item assignment, capacity, objective, and validation.
5. Same-item external and internal flows may share legal physical geometry.
6. No placement, rotation, facility port, boundary terminal, or route is fixed by the harness.
7. A timeout without a complete incumbent remains `unknown`, never `infeasible`.

## Validation And Diagnostics

Independent witness validation must reject:

- a missing or duplicate external terminal;
- a terminal whose node, direction, item, or rate differs from the prepared requirement;
- an external terminal not on the reported used-bounds side;
- terminal flow that violates conservation, item assignment, direction, or capacity;
- shared route geometry that overlaps a facility on the same physical coordinate;
- any ordinary shared-network topology or physical-collision violation.

Diagnostics retain stable stage, severity, code, path, message, and entity fields.

## Cutover Completion Criteria

- external edges are not partitioned out of `ModelInput.networks`;
- the dedicated external-connector model and output types are deleted;
- shared routing builds complete boundary-terminal choices for every external endpoint;
- validation and HTML consume the shared terminal representation only;
- all workspace tests pass;
- release-mode cumulative SCC artifacts exist for success or failure;
- model construction, first incumbent, final objective, variables, constraints, terms, RSS, and
  termination are compared with the canonical-occupancy v3 baseline.
