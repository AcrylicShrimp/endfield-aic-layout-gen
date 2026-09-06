# Catalog-backed crossing-site replanning

Input: a complete validated detail, runtime catalogs, a local move/chain request, and
`maxNewCrossings` (default zero, explicit research limit 0..8). Preserve recipe rates,
facility/terminal identity and existing component topology. A separate improvement family
may add one-cell bridge modules at legal crossings of distinct commodities on one layer.
Port identities stay fixed unless the assignment option below is enabled. No claim is
made that the chosen bridges are the fewest required crossings.

The optional `portAssignments` transaction assigns existing singleton ordinary facility
terminals to compatible runtime ports (at most sixteen assignments). Preserve each terminal
ID, item, rate and owner. Reject occupied ports, shared terminals, components and external
terminals. Simultaneous swaps are checked after the complete assignment, not sequentially.
Reroute every incident/displaced chain, releasing old directional arms at changed ports.
The paired candidate family swaps neighboring equal-rate same-item boundary terminals;
other port permutations are outside this initial proposal family. No extra supply or demand
is introduced. Enable the bounded paired proposal family with `--port-policy paired`;
the default `retained` policy remains an explicit control.

Before routing an affected chain, enumerate candidate bridge poses on the concrete
retained/already rebuilt chains. A site has exactly one straight constant-rate carrier,
no terminal, no existing junction, no facility body, and no reserved attachment arm.
Choose an allowed runtime bridge rotation supporting both independent straight axes and
both exact rates. The incoming chain must traverse its perpendicular axis without turning
inside the component. Existing bridge cells cannot gain a third channel. Same-item
crossing is excluded from this bounded family, not declared illegal in general.

A* may enter a precomputed bridge module through its declared incoming arm; the bridge
state forces the next step straight out of the opposite arm. Adjacent bridges need no
artificial spacer. State includes the number of new bridges used, so a shorter exhausted-
budget prefix cannot hide an admissible longer prefix. Manhattan remains a lower bound.
Returned paths include all actual unit cells; repeated cells are rejected. Geometry must
remain within the current local extent.

Install only chosen modules, attach their IDs to both commodity networks, and preserve
the carrier geometry. Recompute candidate sites after each restored chain. If any remaining
connection fails, discard the whole private move. No component is installed in the incumbent.
The entire candidate must independently validate and improve used area/cells/turns before
promotion. A bridge budget or routing failure is not a proof of local/global infeasibility.

Output diagnostics include candidate site counts, chosen module poses/carrier chain IDs,
route work and the configured/consumed bridge budget. Keep a zero-new-bridge control.
Verify catalog arms, rotations, transport and capacity, blocked anchors, unit paths, resource
budgets, input immutability and whole-factory acceptance. Inspect actual bridge and full-layout
renders, including rejected trials when no complete candidate is found. No Rust cutover.
