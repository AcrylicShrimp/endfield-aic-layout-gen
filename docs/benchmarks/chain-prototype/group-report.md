# Milestone 2: generated atomic cycle and branch

Status: complete for the [group contract](group-contract.md). The full research goal is active.
Production construction and global exact research tooling are unchanged. External factory
closure, multi-input consumers, completion-aware evaluation and completed-region rollback
remain later milestones.

## What is now constructed

`citron-group.json` contains the same four facilities and exact utilized recipe rates as the
citron reference, but no placement, rotation, port or route hints. The constructor derives
the growth order G → P2 → S → P1 from connectivity. It validates the grinder seed, privately
adds and routes the product planter, privately adds and routes the seed collector, then adds
the cycle planter and routes both directions of the cycle before validating that state.

Only two public commits exist: the validated seed and the closed four-facility internal group.
The intermediate private histories are explicitly uncommitted. Each history preserves all
older relative body geometry, ports, directed routes and rates; canonical translation merely
changes the origin. The deepest unfinished private prefix is retained for diagnostics, not
offered as a committed factory. Every introduced facility has all induced internal edges
routed and independently checked before further growth. There is no deferred global routing.

The seed collector produces 1 seed/s, split into two 0.5/s outputs on distinct capacity lanes.
The cycle planter returns 0.5 plant/s; the product planter sends 0.5 plant/s to the grinder.
Both same-item requirements share one logical commodity-network DTO, without falsely forcing
their combined rate through a single 0.5/s trunk. No splitter is required for these distinct
facility output ports. The independent probe reports exactly one pending margin: the grinder's
1 powder/s export. That export still requires two belt-capacity lanes in a later boundary
stage. These are steady-state internal witnesses, not full factories or bootstrap proofs.

## Controlled first-completion results

The control CLI changes only which relative approach is tried first. Graph, inventory, rates,
external obligations, port limit, gaps and work caps remain identical. Each run stops at its
first independently validated complete internal group. Thus the caps are matched, but actual
work differs; this is not the milestone-5 equal-consumed-work optimization comparison.

| First approach | Used size | Area | Physical cells | Segment turns | Proposals | A* expansions | Validations |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Straight | 22 × 6 | 132 | 18 | 2 | 69 | 17 | 4 |
| Left 90° | 14 × 12 | 168 | 34 | 3 | 9 | 293 | 4 |
| Right 90° | 14 × 12 | 168 | 34 | 3 | 9 | 293 | 4 |
| Fold 180° | 7 × 18 | 126 | 19 | 0 | 4 | 19 | 4 |

All four runs pass internal closure and retain `factory_validated: false`. The measured replay
construction intervals are about 18–23 ms on this machine; no portable performance claim is
made. Catalog preparation, report/HTML rendering and CLI startup are outside that interval.
The straight run selects gap 1; the bent/folded runs select gap 0 and have actual body-edge
contact, independently verified in tests. Zero gap does not permit transport inside a body.

Fold-first is smaller than straight-first in this limited comparison, but that is not evidence
to always fold. Left/right first completions take long return routes and are worse. The human
internal reference has area 120 and seven transport cells, so this stage has not reproduced
its short-route quality. The reference was not generated under these work limits and excludes
external export routing too. It is a pattern comparison, not a fair optimizer benchmark.

As in milestone 1, segment-only turns exclude bends between a port arm and its endpoint cell.
Zero segment turns in the 180-degree control do not mean material never changes direction.
Do not compare these turn numbers with the reference's port-arm-inclusive count. A common
metric must be used for the later full matched-inventory comparison.

## Proposal interpretation and confirmed correction

The user's additional observations were incorporated as geometry alternatives: a bent pair
can have touching bodies when its actual connection remains clear, and a chain may turn back
180 degrees, though that is less common. The default request keeps folding last. Choices are
made per connection; they are not rigid chain macros. A synthetic rejection regression forces
a left-then-straight continuation and proves that private retries can mix approaches. This
test-only rejection is not a game constraint or a demonstrated quality heuristic.

The initial zero-gap implementation failed its new test: it moved the connection point one
cell into the consumer while reducing body spacing. The independent geometry checks rejected
those candidates, and left-first selected gap 1 instead. The verified correction keeps the
connection outside the consumer while reducing the tangential body gap. All bent/folded
controls then select legal body contact. No search budget was increased to hide the omission.
A rotation test initially used the opposite traversal sign; inspection confirmed the generated
poses and the expectation was corrected, without changing valid geometry.

## Failure and evidence contract

Shared proposal, route-expansion or validation exhaustion retains only the committed seed,
plus a clearly labeled uncommitted three-facility private prefix. A late validator result is
not accepted. Rejected private geometry cannot be expanded; a rejected closing result cannot
commit. Closing-port alternatives retain separate route/validation artifacts, including the
rejected attempt. Graph-margin rejection remains distinguishable from validator-process failure.

Removing the cycle edge can still pass the independent partial-geometry probe; the graph
closure gate catches the missing induced requirement. Malformed ledgers, fixed geometry,
parallel edges, unsupported cycles/merges and invalid policy reject without silently cutting
the graph. Renamed IDs, including `__proto__`, and reversed input arrays preserve geometry and
deterministic work. Reusing an output directory rejects without modifying the earlier report.

The algorithm tries unused closing ports in geometric distance/position order after extending
the logical network. This is a local spatial baseline, not joint assignment of all future ports.
Private retries are confined to the current transaction. Seed orientations are separately
validated empty-to-seed trials, not movement of unrelated committed regions. General committed
region rollback and completion-aware quality ranking remain unimplemented.

## Reproduction and verification

```sh
cargo build --release -p aic-cli --bin aic-witness-probe
node docs/benchmarks/chain-prototype/run-group-controls.mjs \
  --request docs/benchmarks/chain-prototype/citron-group.json \
  --recipes data/game/normalized/recipes.json \
  --facilities data/game/normalized/facilities.json \
  --items data/game/normalized/items.json \
  --transports data/game/normalized/transports.json \
  --components data/game/normalized/logistics-components.json \
  --validator target/release/aic-witness-probe \
  --output-directory target/chain-m2-citron-replay
node --test docs/benchmarks/chain-prototype/*.test.mjs \
  docs/benchmarks/user-layout-reconstruction/*.test.mjs \
  docs/benchmarks/constructive-*/*.test.mjs
cargo check --workspace
cargo test --workspace --quiet
```

Use `run-group.mjs` with the same flags for a single request, without the controls wrapper.
All output directories must be fresh. Final control artifacts are under
`target/chain-m2-citron-controls-release/`. Each approach directory contains its exact derived
request and a `run/` directory with `report.json`, `base-witness.json`, `selected-witness.json`,
`index.html`, and immutable per-validation witness/decision/process/report/log directories.
Reports hash runtime catalogs, input, validator and implementation; the controls report also
hashes its source request, runner and child reports. Earlier `-initial` artifacts are exploratory.

All 12 group tests and 12 chain tests pass. The broader Node suite passes 316 tests. Rust
workspace check/tests pass: 524 passed, two ignored. No Rust or dependency files changed.
Browser checks cover all four pages at three viewport widths and two themes (24 cases),
including focus, zoom, pan, fit and overflow. Final browser artifacts are under
`target/chain-m2-<approach>-release-browser/`. Straight, left and fold full views were personally
inspected; right is also covered by the browser checks.

## Policy audit and next step

The implementation follows the accepted constructive planner's private, incrementally routed
closed-group transaction policy. It uses external game data, explicit pending boundaries, no
power, no caller blueprint W/H, and the dedicated router. No production or exact-research
fallback is introduced. The shared graph-ledger parser and work counter were extracted from
the prototype only; milestone-1 controls retain their existing geometry and behavior. Unrelated
dirty files are preserved. No custom propagator or production architecture cutover occurred.

Next: define multi-input consumer composition and explicit boundary-routing contracts, then
construct the complete battery, citron and enriched-iron factories. Keep nine- and eight-facility
iron inventories separate. After complete acceptance is available, evaluate selective bends,
port ordering and remaining-space estimates through actual routed continuation and bounded
rollback. The current controls establish a working cycle/branch transaction, not completion
of the overall research goal or reproduction of the user's layout quality.
