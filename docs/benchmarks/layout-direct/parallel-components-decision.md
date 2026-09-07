# Parallel production components: construction policy decision

Status: proposed; requires user approval before implementation.

## Evidence and present boundary

The broadened target-load run requests small, medium and large factories explicitly. It
completes one seven-facility training group, then exhausts 200 attempts for the required medium
slot. All 11 distinct medium construction attempts contain 2–16 internally disconnected
production components. In every case, the constructor completes the seed's internal component,
has no remaining internal edge incident to placed facilities, and performs zero placement or
route search at the failure. Two preceding small requests fail the same way. None is a timeout.

The current frontier in `constructive/production.rs` follows only facility-to-facility edges
incident to the current component. The runtime wiring legitimately contains parallel instances
and independent chains, often sharing only declared external input or final-product nodes.
There is no missing material balance and no permission to delete parallel instances, import
their intermediate outputs, reduce target rate, or insert fictitious internal requirements.

AGENTS and the accepted constructive design require growth of one connected production subgraph
through an immediately routed frontier facility or atomic group. Do not silently reinterpret
that rule as permission to place disconnected seeds or treat arbitrary whole components as the
already approved downstream-closure exception. Shared-commodity embedding remains a possible
future strategy, but forced mixing is not required by these original point-to-point obligations.
This evidence establishes a limitation of the current frontier, not global impossibility.

## Proposed explicit extension

Permit multiple independent internal production components in one factory request, while keeping
the existing connected, facility-by-facility placement and immediate routing rule *inside each
component*. Admit a new component as a separately reported transaction; never globally place all
facilities first and route afterward. The original full request remains the source of truth.

Inputs: original instance wiring, loaded catalogs, target seed preference, construction budgets
and deterministic internal-component partition. Partitioning must not modify facility identity,
recipe, utilization, commodity rates or any original internal/external requirement.

Outputs: one flattened factory witness, component membership/order and per-component growth
history, exact unresolved original requirements, separate internal and final-boundary work,
and independent whole-request coverage/geometry validation. Component identity is diagnostic,
not an immutable placement macro in the subsequent learner.

Required invariants:

- Build and route each component incrementally. Preserve unrelated accepted internal geometry;
  failed new-component work rolls back without erasing earlier components.
- Charge component construction, spatial admission and global external-boundary completion to
  explicit existing/newly specified transaction budgets. Natural W/H remains an output.
- Before full acceptance, place and route all original external inputs, target outputs and
  surplus outputs on the actual combined factory boundary. A locally external terminal hidden
  inside the combined layout is not valid merely because its component was valid in isolation.
- Revalidate all original rates, coverage, capacity, reachability and final geometry. Do not
  report a collection of individually valid components as a complete valid combined factory.
- The improver receives ordinary facilities/networks with no frozen component internals. Best
  retention keeps the original area/tile/turn score order and no exact-solver fallback.

Failure modes: incomplete component, no safe spatial admission, exhausted boundary routing,
budget exhaustion, or original-request validation failure. Return structured evidence and the
last valid state for its declared covered scope, never a false complete factory.

This proposal is not yet an accepted architecture change. If approved, update the AGENTS/design
policy explicitly, define the precise transaction DTO/report version, implement and verify the
new constructor slice, then rerun the original failed requests and stratified learning stream.
