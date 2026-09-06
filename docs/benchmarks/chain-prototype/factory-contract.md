# Milestone 3: multi-input composition and external factory closure

Status: implemented and verified; see the [milestone-3 report](factory-report.md) for conditional
completion results and limitations. This contract was established before implementation.
Separate experimental construction and verification commands; production generation and the
independent Rust probe are unchanged.

## Input DTO and supported graph

Factory request schema 2 retains geometry-free nodes (`id`, `recipe`, exact `utilization`),
internal edges (`id`, `source`, `target`, `item`, exact `rate`), and explicit external obligations
(`id`, `instance`, `direction`, `item`, exact `rate`). The full ledger must exactly match every
utilized recipe margin. Runtime catalogs supply all game data; no screenshot pose, selected
port, route corner, caller blueprint width or height is accepted.

The supported graph has a unique singleton sink and a connected upstream dependency tree
after collapsing reciprocal two-node cycles. A component may have multiple suppliers but one
downstream consumer component. Inside each two-node cycle, both directions must be present;
parallel internal edges, self-loops, larger cycles and general reconvergent graphs remain
explicitly unsupported. Internal edges fit one capacity lane. External obligations may span
multiple capacity lanes and must be split exactly, without changing their aggregate rate.
This stage supports one transport kind per request; mixed-kind scheduling remains unsupported.

`policy` contains ordered `families` (straight/left/right/fold), integer `gaps` including zero,
`ports_per_side`, `slides_per_pose`, and `port_order` (spatial or center). Sliding alternatives
come from retained body edges, not fixed absolute offsets or a frozen full placement. Spatial
port order follows actual edge positions. Upstream chains are ordered by structural depth,
commodity and catalog-backed structure, with identity only a deterministic indistinguishable
tie-breaker. No chosen order proves omitted configurations illegal.

`boundary` contains positive `margins` and an `endpoint_limit`. These control bounded perimeter
routing trials around the constructed internal geometry. They are neither blueprint dimensions
nor game limits. `budget` retains the shared proposal, route-expansion, validation and elapsed
limits. All construction, port/pose retries and boundary routing charge the same work account.

## Growth and commit contract

Validate the sink seed, then expand one supplier component at a time. A singleton transaction
adds and routes one facility. A cycle transaction introduces its facilities incrementally in
private, routing all newly induced edges before introducing another. Commit only when all
internal outputs of the covered graph are closed. Failed attempts preserve the last committed
state and distinguish private diagnostic prefixes from public states. No global facility-only
placement, automatic exact-solver fallback or unrelated-region relocation is permitted.

A completed internal layout may still have its originally declared external obligations pending.
Boundary routing is a private transaction after internal growth; if it fails, retain that
validated internal layout and report incomplete external closure. The initial constructor takes
the first validated transaction, not the lowest partial area or a claimed best final layout.
Quality lookahead and bounded rollback of committed chain windows remain milestone 4.

After initial experiments identified routes occupying all future input ports, a pending-port
access heuristic is tested before accepting growth. For each covered facility/direction, enough
unused ports must reach outside the current geometry through free same-layer cells to cover
the original unrouted lane demands. Flood work is charged to `route_expansions` and reported
separately from A*. This may omit legal future facilities inside a closed pocket; it is not a
sound infeasibility rule or proof of future-body fit. No work budget increase accompanies it.

The battery control exposed a further shared-channel issue: three individually reachable input
ports depended on one narrow entrance. The next controlled heuristic also asks each counted
future port for a free outward stub whose length equals that facility/direction's pending lane
count. This is an explicit conservative multi-lane clearance proposal rule, not a game minimum
gap or a proof that other routed arrangements are impossible.

## External-boundary semantics

Each obligation has its own explicit external identity derived from its request ID. An input
obligation becomes an external output terminal feeding its facility input; an output obligation
becomes a facility output feeding an external input terminal. Each physical lane has a unique
terminal pair and an exact rate no larger than catalog capacity. Compatible lanes share one
logical item/kind network DTO. No undeclared source, surplus sink or internal cut is allowed.

For each perimeter trial, derive a rectangle from the actual internal geometry and the selected
margin. Route boundary lanes within that rectangle so existing external terminals cannot cease
being extremal when later lanes are added. Each external endpoint must lie on the **actual used
geometry's** corresponding extreme side; unused trial capacity is not part of the score. Route
every lane to a legal unused facility port with the dedicated router. Canonical translation
preserves all geometry, side directions and identities. Power and outside-factory transport
are excluded; no claim is made about game bootstrap inventory or a specific warehouse layout.

## Independent acceptance and output

The Rust probe remains a read-only geometry/recipe checker. Its `factory_validated: false` is
not overwritten: it does not own the original production request. The independent factory
verification command loads the original request, runtime catalogs and an arbitrary witness,
runs that probe, and separately checks exact inventory/utilization, all induced internal
requirements, declared external identities/rates and zero pending recipe margins. It does not
import the constructor or trust a producer-supplied success flag/history. Factory acceptance
requires both checks. Reports expose the constituent results and their scopes explicitly.

An internal requirement ID remains unique across the graph. External requirement IDs are also
unique, but may own multiple explicitly identified lanes. Terminal identities use structured
kind/requirement/lane tuples so arbitrary graph IDs cannot alias internal and external records.

Construction reports include committed history, private retries, chosen component/chain order,
poses/port choices/slides, route bounds/work, independent artifacts, boundary lane decisions,
pending obligations and the last accepted witness. Only a fully accepted factory has a final
score: used area, physical transport cells, then segment turns. Incomplete internal states are
not ranked against complete factories. The later cross-reference comparison must unify turn
definitions before making a fair quality claim.

Diagnostics use stable `chain-` codes and include severity, stage, message and entity. They
distinguish unsupported graph/transport, port or placement omission, routing failure, invalid
coverage/boundary semantics, independent validator failure and exhausted work. A local failure
or resource limit is never global infeasibility. CLI artifacts are immutable and provenance-hashed.

## Verification gates

- Complete battery (eight facilities), citron powder (four), and observed enriched iron (nine)
  from geometry-free graphs; keep minimal eight-facility iron as a separate request/comparison.
- Immediate routing at every introduced facility; only closed cycles commit; unrelated accepted
  geometry survives current-transaction failure. External lanes retain exact aggregate rates.
- Independent full verification from original request plus witness, including negative controls
  for missing/extra facilities, internal edges, external lanes/identities, rate changes and
  non-boundary endpoints. A partial-geometry pass alone cannot imply factory acceptance.
- Trace spatial input-chain/port ordering and slide proposals. Preserve straight/contact/fold
  choices; do not encode screenshot positions or assume a diagnostic canvas is a game bound.
- Matched-cap controls, reproducible CLI runs, focused and broader tests, workspace verification,
  rendered inspection, a measured slice report and a focused commit complete this milestone.

If growth stalls, inspect the failing port/body/route trace and compare it to the user's parallel
supplier chains and future-consumer examples. Test a small proposal/port-reservation hypothesis
before enlarging work limits. Full completion-aware comparison remains the subsequent goal.
