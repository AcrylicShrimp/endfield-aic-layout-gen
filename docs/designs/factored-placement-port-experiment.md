# Factored Placement And Port Experiment

## Status

Accepted research experiment. This contract does not replace the dense production solver or the
flattened shared-layer research formulation.

## Question

Can the exact shared transport-layer model replace each Boolean state for a Cartesian
`placement candidate times port` pair with separate placement and port state variables, while
preserving every legal joint placement, rotation, directional port, and routing solution?

## Reference Encoding

For every logical facility endpoint, the current exact encoding creates one Boolean variable for
each legal pair of:

- a facility placement and rotation candidate; and
- a compatible facility port.

The selected pair directly identifies the connection cell and the only legal arm direction. This
is exact, but it repeats the same placement state for every endpoint and port.

## Factored Encoding

The experimental encoding keeps the existing exact facility-placement choice and adds:

- one integer port-choice variable for each logical facility endpoint;
- one integer terminal-geometry variable identifying the selected connection cell and arm
  direction; and
- an exact allowed-tuples constraint relating placement choice, port choice, and terminal geometry.

For an external endpoint paired with a facility endpoint, the same placement and port choice
produces the external terminal geometry with the opposite arm direction. No external coordinate is
selected independently.

Routing constraints consume equality literals derived from terminal geometry. These literals are
derived state, not placement-port candidates: one literal represents one reachable physical
cell-direction result even when several placement-port tuples produce it.

## Exact Semantics

The allowed-tuples relation contains every placement-port tuple whose outward connection cell lies
inside the caller's hard bounds. It contains no other tuple. Therefore the experimental encoding
must preserve:

- every legal facility origin and rotation;
- every compatible input or output port;
- the port's rotated boundary side and single legal connection direction;
- the exact facility or external terminal cell;
- joint placement, port selection, item assignment, and routing search; and
- the complete lexicographic objective.

No port, placement, direction, coordinate window, route corridor, or candidate order may be fixed
or removed heuristically.

## Comparison

The primary comparison uses the same shared belt/pipe layer formulation on both sides:

1. flattened placement-port endpoint Booleans; and
2. factored placement, port, and terminal-geometry variables.

Each formulation receives an independent equal search budget. The report must record variable
families, domain volume, constraint structure, placement-routing coupling, construction time,
first-incumbent time, objective, termination, and validation.

## Validation

Before the Heavy Xiranite phase-zero run, small multi-item cases must show that the dense reference
and factored shared-layer formulations:

- both produce independently valid witnesses;
- have the same lexicographic objective; and
- preserve selected port identity, connection cell, and direction.

The research command must write comparison JSON and both self-contained HTML views even when a
formulation times out without an incumbent.
