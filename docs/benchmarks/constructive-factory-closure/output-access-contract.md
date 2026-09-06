# Preserve pending output access during growth

Input: current normalized graph and catalogs, private candidate witness after routing
the added facility's inputs, projected catalog ports and its occupancy index.

Compute exact per-facility/per-item output margins from the original graph, subtract
already routed facility-output terminal rates, and sum the minimum remaining lane
counts by facility and transport. Different items require separate ports. Compatible
fragments are aggregated before rounding up to runtime transport capacity. Used ports
are not available to this fixed-interior closure router.

For every positive pending margin, require enough unused output connection cells that
are nonnegative and blocked by neither a facility nor same-layer transport/component.
Different-layer occupancy does not obstruct access. Do not reserve unused ports on
facilities with no remaining obligations. Inspect all retained facilities, not only
the newly added one: its body and input routing can obstruct an older frontier.

Reject a failed private candidate before materializing or accepting it. Emit
`closure-pending-output-obstructed`, severity error, stage factory-closure, with owner,
transport, pending items/rates, required/available counts and per-port obstruction.
Keep the accepted base untouched and try the next candidate under the existing bound.

This is necessary immediate access for the current unused-port/fixed-interior family,
not sufficient end-to-end routability. It does not prove global infeasibility, reserve
whole straight corridors, perform port reassignment or permit implicit same-layer
sharing. No geometry or capacity validation is weakened. Actual local and complete
independent validation remain required. The production Rust constructor is unchanged.

## Closed-pocket footprint lookahead

After immediate connection access passes, flood the candidate's free same-layer space
from its remaining output ports. Reuse connected-component results within the private
candidate. Stop at the measured geometry's perimeter or an explicit 4,096-cell work
limit; either outcome remains unknown/allowed, never a closed-space proof.

For a fully explored closed pocket, enumerate actual allowed consumer footprint sizes
from runtime catalogs. Require at least one placement wholly inside the free component,
excluding the selected source connection and all existing body/belt/pipe cells. A free
region with sufficient area but the wrong shape must not pass. Each pending target
needs at least one eligible source port; this is not a simultaneous packing or routing
proof. Actual consumer port orientation, path and multi-input access still require
later construction and independent validation.

An external export requires perimeter access; a completely closed pocket cannot qualify.
Emit `closure-pending-consumer-no-space` with source, target, transport, allowed sizes
and the per-port flood/fit evidence. Reject routed overproduction separately with
`closure-output-margin-exceeded`. The current experiment does not relocate retained
facilities or replace retained wires with crossings to open a failed pocket.
