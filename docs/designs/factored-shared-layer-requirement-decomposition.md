# Factored Shared-Layer Requirement Decomposition

## Status

Accepted follow-up diagnostic. It narrows the first boundary found by the factored-network
decomposition and does not change production solver semantics.

## Question

Is either logical route requirement inside the Xiranite Powder commodity network intrinsically
hard, or does the five-second validated-output cliff appear only when both requirements share one
solver-selected physical network?

## Controlled Formulation

Every case uses `joint-shared-transport-layer-factored-endpoints-v1` with unchanged placement,
rotation, port selection, shared-layer routing, flow, topology, collision, and objective rules.
No solver decision is fixed and no constraint family is removed.

## Case Construction

The selected phase-zero commodity network is rebuilt from:

1. each individual logical route requirement; and
2. the complete set of requirements in that network.

Each subset is rebuilt from `EdgeInput` before network normalization. Excluded requirements must not
leave terminal, endpoint, flow, or topology variables in the model.

## Measurement

Each case receives an independent five-second release-mode budget. JSON and self-contained HTML are
written for every outcome. The report compares endpoint state, variables, constraints, terms,
factor incidences, placement-routing coupling, construction time, first incumbent, validation, and
termination.

## Next Boundary

If both one-requirement cases are tractable and the combined case is not, the next iteration will
decompose the added shared-network constraint families. Any relaxed or fixed-decision case will be
labelled diagnostic-only and cannot be treated as a valid layout result.
