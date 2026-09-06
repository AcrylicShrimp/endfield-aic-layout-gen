# Milestone 2: atomic cycle and branch growth

Status: implemented; see `group-report.md`. The contract was established before implementation.
This is a separate experimental entry
point, not a change to production construction or a complete external factory.

## Input and scope

The group CLI uses the same geometry-free node, edge, external-obligation and budget DTOs as
milestone 1. Schema 1 is scoped by the explicit group entry point. Runtime catalogs supply all
game data. Its supported graph is one two-facility directed cycle, one outgoing branch from
that cycle, and a simple downstream chain ending at a unique sink. Parallel requirements,
self-loops, additional cycles, multi-input consumers and edges exceeding one lane capacity
are rejected as unsupported, not globally infeasible. Every recipe margin must exactly equal
the declared internal and external rates. No coordinates, selected ports or canvas are inputs.

`policy.families` now orders alternative relative approaches at **each** growth step, rather
than forcing a repeated turn throughout a chain. In addition to straight/left/right, `fold`
proposes a 180-degree return with aligned input/output faces and bodies on either tangential
side. The user identified this as legal but less common; the control request orders it last,
not as an always-beneficial shape. `gaps` permits zero for bent body-contact
proposals; zero does not mean a transport cell can pass through a facility. Straight zero-gap
proposals with blocked connection cells are rejected normally. `ports_per_side` bounds the
catalog-derived spatial choices. No minimum spacing is asserted as a game rule. This stage
supports a single transport kind; mixed pipe/belt group scheduling remains explicitly unsupported.

## State and transaction invariants

Derive the sink-first order from connectivity, traversing the cycle only once. The sink is the
committed base. The other connected facilities form one private downstream-closed transaction.
Introduce exactly one facility, choose its required ports, extend compatible logical commodity
networks, and route **all** edges between it and existing members before introducing another.
The closing cycle facility therefore routes both directions before its private state validates.

The independent Rust witness probe checks geometry, rates and capacities. An additional graph
check requires exactly the induced internal edges and their expected pending margins. A private
state may still have explicitly known output obligations to uncovered group members; it is not
a committed closed group. Commit only the complete group, with every internal output closed
and only declared external margins pending. Steady-state cycle validation is not proof of
bootstrap inventory or game operation. External export routing is a later milestone.

Within the private transaction, try deterministic relative pose and port alternatives. Each
validated private prefix may be continued; a failed continuation discards only that private
suffix and retries the earlier choice. This is transaction-local retry, not relocation of
unrelated committed regions or milestone 4's completed-factory improvement. The base is never
modified. Retain the deepest independently checked private prefix as a diagnostic artifact,
clearly uncommitted, if the group fails. No facility-only global placement is allowed.

One logical network groups compatible item/kind requirements before embedding. Separate
capacity lanes from distinct facility ports are permitted when the sum cannot fit one trunk;
the citron seed output is such a case. This slice does not invent splitters or claim shared
trunk optimization. The directional physical routes and aggregate recipe ledger remain checked.

## Work, decisions and output

The existing proposal, route-expansion, validation and elapsed-time limits are shared across
seed orientations, placement choices, closing-port trials and private retries. Each seed
orientation starts a separate empty-to-seed trial, not relocation of an existing region;
seed trials charge validation work. A growth pose attempt
and a closing-port pair each consume one proposal unit. Every search expansion and independent
validation is charged. The first complete validated group is retained; this is a feasibility
baseline, not completion-aware quality selection or an optimality claim.

Report schema 1 includes status, committed base, committed full group if any, private history,
deepest uncommitted prefix, decision trace, input/implementation provenance, budget and structured
diagnostics. Decisions identify parent artifact, facility, introduced edges, relative approach,
gap, pose, ports, local route bounds/work, validation, retry and commit outcomes. A failed group
has no completed score. A success reports area, physical cells and segment-only turns, with
`factory_validated: false`. The previous full-factory goal remains unchanged.

Stable `chain-` diagnostics distinguish unsupported topology/transport, proposal overlap,
unavailable ports, local route failure, invalid graph closure, independent validation failure,
and exhausted work. They include code, severity, stage, message and entity. A failed local
route or transaction is never a whole-factory infeasibility proof.

## Verification gates

- Geometry-free four-facility citron graph at exact full-utilization rates; no screenshot poses.
- Each private state adds one facility and routes every induced edge before further growth.
- Both cycle directions close, seed output splits without duplicated flow, final pending
  margins equal only the declared powder export, and the independent probe passes.
- No open cycle/branch state is publicly committed; route/validation/budget failure retains
  the committed base and distinguishes private diagnostic geometry from a valid full group.
- Reordered arrays and renamed IDs preserve behavior; invalid ledgers and unsupported topology
  reject without repairing the request. Zero-gap proposals are observable and geometrically checked.
- Reproducible CLI artifacts, focused regression tests, broader tests, workspace verification,
  rendered inspection, a measured report and a focused commit complete the milestone.

If first attempts cannot close, inspect the source examples and distinguish omitted poses,
poor port assignment, route obstruction and work exhaustion. Test one small mechanism change
before increasing budgets. Do not encode the screenshot's absolute geometry as a fallback.
