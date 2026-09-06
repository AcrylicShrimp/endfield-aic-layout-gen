# Milestone 4: bounded connected replay and completion evaluation

Status: complete for the [replay contract](replay-contract.md). Milestone 5's equal-total-work
comparison and final recommendation remain open. Production and global exact research are
unchanged. This is a fallible search strategy, not universal completion or proof that body-fit
ranking always improves quality.

## Implemented contract

`run-replay.mjs` takes the unchanged geometry-free factory request and a separate search policy.
Inventory, utilized recipes, internal requirements and external lanes keep their M3 acceptance
contract. Neither screenshots nor guessed final width/height enter construction.

The existing growth/routing/access/validation logic now lives in `factory-kernel.mjs`, yielding
validated singleton or atomic-cycle transactions. M3 selects the first; M4 shortlists and
replays. The former implementation was removed from `factory.mjs`. What is simpler: both use
one growth invariant, without reconstructing routes from reports or calling another generator
as a fallback. Regression tests preserve the four selected M3 geometries and exact work counts.

The shortlist keeps at most one completed transaction per initial relative family, up to its
configured size. Exhaustive order tries each family in turn; interleaving alternates their
existing proposals. Once a representative is accepted, that family's generator stops. Later
port/pose variants in that family are deliberately omitted by this bounded policy.

`partial-area` orders the shortlist by routed prefix score. `remaining-fit` first orders it by
a greedy estimate of accommodating the missing catalog-backed bodies against the frozen body
and transport mask. It tests outward growth on all four sides and scans current-envelope holes;
each temporary body blocks later temporary bodies. These poses never enter construction,
routing or validation. The estimate omits future ports/routes and is neither a minimum nor a
factory proof. Exhausted estimates have `estimated_area: null` and use partial-area fallback.

Chosen candidates receive actual incremental routed continuation and external closure. Trial
ancestry links them to their starting candidates. Only complete independently checked scores
can replace the complete incumbent, strictly improving area, physical cells, then turns.
The deepest validated output-closed prefix is separately retained before any completion.
Failed, late, worse or equal-score trials cannot replace a complete incumbent.

Replay restores the snapshot before a bounded recent suffix. Removed nodes must be connected,
respect the facility-count limit and include whole cycles. The seed is frozen. An episode's
floor is fixed at its first terminal trial; it cannot walk backward after repeated failures.
Earlier bodies, utilization, ports and internal routes are checked for exact preservation
modulo translation. The changed used frame owns the external-perimeter closure transaction,
which is privately replayed in full. No unrelated internal route is ripped up. This is not
arbitrary-region relocation or unrestricted global DFS.

## Work and the first correction

Each request retains the M3 global caps: 1,024 proposals, 200,000 combined A*/access expansions,
512 validations and 30,000 ms. A frontier or boundary gets a slice of 128 proposals, 25,000
expansions and 32 validations inside that shared account, never a fresh global budget. The
policy permits eight routed trials, four rolled-back facilities and four shortlisted families.
Fit has 8,192 shared checks and a 1,024-check slice per candidate. Both ranking controls receive
that allowance; Partial uses none. Fit time is charged; CLI preparation/rendering is outside
construction time. Local slice exhaustion and global exhaustion have separate diagnostics.

Initially the shortlist discarded additional representatives of an already represented family
only after validating them. This crowded out other families. Stopping that generator at its
first representative reduced the same failed battery run from 659 proposals/102 validations
to 572/18. Newly available families changed citron area 192 → 162, iron-nine 391 → 340 and
iron-eight 304 → 285. These are the `-initial` versus `-family-stop` experiments. No cap increased.
The cycle generator also stops its remaining private choices when its root family is filled.
This is an enumeration policy, not a sound pruning rule.

## Final proposal-order by ranking controls

`E` means exhaustive family order, `I` interleaved; Partial/Fit are the ranking controls. Each
row gets the same original graph and global limits. Actual work differs. Four controls do not
constitute one free portfolio, and their axes differ from M3's first-family matrix.

| Inventory | Order / rank | Complete area / cells / turns | Trials / replays | Proposals / expansions / validations / fit checks | Termination |
| --- | --- | --- | --- | --- | --- |
| Battery 8 | E / Partial | 220 / 38 / 12 | 2 / 1 | 694 / 6264 / 37 / 0 | Window; incumbent kept |
| Battery 8 | E / Fit | — | 1 / 0 | 572 / 6453 / 18 / 2201 | 7 retained; window |
| Battery 8 | I / Partial | 220 / 38 / 12 | 1 / 0 | 566 / 4688 / 30 / 0 | Window; incumbent kept |
| Battery 8 | I / Fit | — | 1 / 0 | 572 / 5830 / 18 / 2316 | 7 retained; window |
| Citron 4 | E / Partial | 228 / 25 / 6 | 8 / 7 | 422 / 4462 / 50 / 0 | Trial limit |
| Citron 4 | E / Fit | 228 / 25 / 6 | 8 / 7 | 422 / 4462 / 50 / 125 | Trial limit |
| Citron 4 | I / Partial | 162 / 19 / 7 | 8 / 7 | 171 / 6477 / 55 / 0 | Trial limit |
| Citron 4 | I / Fit | 162 / 19 / 7 | 8 / 7 | 171 / 6477 / 55 / 125 | Trial limit |
| Iron 9 | E / Partial | 364 / 35 / 9 | 7 / 6 | 710 / 39400 / 56 / 0 | Window |
| Iron 9 | E / Fit | 364 / 35 / 9 | 7 / 6 | 710 / 39400 / 56 / 3799 | Window |
| Iron 9 | I / Partial | 340 / 40 / 12 | 8 / 7 | 804 / 28448 / 70 / 0 | Trial limit |
| Iron 9 | I / Fit | 340 / 40 / 12 | 8 / 7 | 804 / 28448 / 70 / 3947 | Trial limit |
| Iron 8 | E / Partial | 312 / 43 / 11 | 8 / 7 | 554 / 6725 / 63 / 0 | Trial limit |
| Iron 8 | E / Fit | — | 5 / 4 | 1024 / 8903 / 21 / 3524 | 6 retained; global proposals |
| Iron 8 | I / Partial | 280 / 29 / 10 | 8 / 7 | 700 / 19392 / 64 / 0 | Trial limit |
| Iron 8 | I / Fit | 285 / 37 / 12 | 8 / 7 | 738 / 26155 / 67 / 2778 | Trial limit |

Thirteen of sixteen controls complete, covering all three families and both iron inventories.
The supplied I/Fit default still fails battery; no request-specific dispatch silently changes
it to Partial. The thirteen-versus-eleven M3 count is not a fair success-rate improvement claim:
the experimental axes and search costs differ. M5 still must compare total-work allocations.

## Actual completion and preservation evidence

Iron-eight I/Partial produces a generated, independently validated ranking reversal:

| Trial | Routed internal area | Complete area | Complete cells | Complete turns |
| --- | --- | --- | --- | --- |
| 1 | 230 | 288 | 29 | 11 |
| 3 | 240 | 280 | 29 | 10 |

The larger internal candidate wins after actual external routing and replaces the incumbent.
This is not the earlier screenshot packing relaxation. Citron I/Fit similarly replaces
176/19/6 with 162/19/7: area retains priority despite the extra turn. Iron-nine I/Fit replaces
340/40/15 with 340/40/12, exercising the turn tie-break only after area and cells tie.

Iron-eight replay removes G2/F2 while freezing T, GP, P, S, G1 and F1, including ports and routes.
Both reintroduced facilities route and validate in order. Citron replays S/P1 atomically and
may widen to connected P2/S/P1 within its fixed floor. Battery E/Partial keeps its first complete
factory when the later G3 alternative exhausts its boundary slice. Tests verify these behaviors
on real witnesses, and explicitly reject partial-cycle rollback outside the main snapshot loop.

## Why the fit heuristic is not enough

A body-only regression uses the same 9×4 envelope and occupied-cell count for two masks. The
remaining catalog-backed 3×3 body fits one at estimated area 36, while a fragmented mask makes
the greedy estimate grow to 48. This proves shape-sensitive accommodation, not factory validity.

On battery, Fit prefers straight F1 with estimated completed body area 112 over a right bend at
120, even though their routed prefix areas are 91 and 66. Its chosen continuation stops at G3.
The final G1/G2 supplier nodes are disconnected without their frozen consumer, so the strict
connected removable suffix includes only G2 and has no alternative. The episode cannot reach
earlier C/F choices. Spare global work is not permission to widen that contract or call the
recipe infeasible. E/Fit iron-eight also fails while Partial completes. Fit adds work without
changing citron/iron-nine outcomes and is worse on the other measured controls.

The remaining weakness combines approximate evaluation, limited candidate coverage and replay
ownership, rather than a missing straight/bent pattern. Body accommodation does not check
simultaneous future port corridors or boundary access. Earlier lookahead or a precisely scoped
fan-in window with frozen connecting anchors are research options, not implemented features.
The evidence does not support increasing work or promoting this Fit ranking to production.

## Common scoring and validation scope

M4 uses `transport-cells-including-facility-arms-v1`: right-angle directed transitions at
transport cells include facility-port arms; transport beyond external terminals is excluded.
Physical cells distinguish belt/pipe layers. Junction scoring is explicitly unsupported.
Measured bounds omit unused perimeter capacity. This reproduces the M0 reference turn counts
7, 5 and 12. Historical M3 segment-only scores remain unchanged and must be rescored in M5.
A reference test initially used generated IDs for differently named reference instances; it
was corrected to load the reference's own catalog-backed ports, without changing the metric.

All four selected artifacts pass the separate verifier with the original request. Rust
geometry/recipe coverage and independent original inventory/rate/internal/external coverage
pass with zero pending margins. The Rust probe's own `factory_validated: false` is unchanged
because it does not own the request. Scope excludes power, bootstrap and a specific warehouse.

## Reproduction and artifacts

```sh
cargo build --release -p aic-cli --bin aic-witness-probe
node docs/benchmarks/chain-prototype/run-replay-controls.mjs \
  --request docs/benchmarks/chain-prototype/battery-factory.json \
  --policy docs/benchmarks/chain-prototype/replay-policy.json \
  --recipes data/game/normalized/recipes.json \
  --facilities data/game/normalized/facilities.json \
  --items data/game/normalized/items.json \
  --transports data/game/normalized/transports.json \
  --components data/game/normalized/logistics-components.json \
  --validator target/release/aic-witness-probe \
  --output-directory target/chain-m4-battery-controls-replay
```

Use `run-replay.mjs` with the same flags for one policy. Substitute `citron`, `iron-nine` or
`iron-eight` for other requests. Every output directory must be fresh. Exit 0 means a complete
incumbent, 2 means stopped without one, and 1 means input/CLI failure. The control runner writes
explicit policies and includes failures. Verify emitted witnesses with the original request
using `verify-factory.mjs` as in the [M3 instructions](factory-report.md#reproduction-and-artifacts).

Final runs are `target/chain-m4-<name>-controls-release/<order>-<ranking>/run/`. Reports contain
policy/catalog/source hashes, immutable probe artifacts, shortlist/fit evidence, trial ancestry,
global/slice work, rollback ownership, termination and incumbent history. A retained internal
witness is distinct from the selected complete witness. Only complete runs emit
`factory-witness.json` and `index.html`. Earlier `-initial`, `-family-stop` and `-first-controls`
artifacts retain historical implementation hashes.

| Independently verified and visually inspected selection | Used size | Witness SHA-256 |
| --- | --- | --- |
| Battery E/Partial | 11 × 20 | `b66159476fc624d4c3982e85278aecc2087927d841b0921b77e6b3fa5c595e85` |
| Citron I/Fit | 18 × 9 | `a8e29c9cc4f0979cc4f570240d9c620401977f32a8b16adec9ad0101e2b930dc` |
| Iron-nine I/Fit | 17 × 20 | `2fd15ada5facacec5e7d20512bba806e5e9ac15fccc10cfdef54f9d6f3f14e95` |
| Iron-eight I/Partial | 14 × 20 | `82efd52f2dc8cdeccc59500b44e97a0a9f54f254b9d07dddc8913d575423c7ab` |

Separate checks are `target/chain-m4-<name>-release-verification/report.json`. Browser reports
and screenshots are `target/chain-m4-<name>-release-browser/`. All four full screenshots were
inspected: labels, boundary endpoints and factory-versus-fit scope are readable. Long seed
return routes and internal pockets remain visibly poor; human compactness is not reproduced.

## Verification and next gate

Fifteen replay tests cover M3 geometry/work; full factory coverage and shared accounting;
immediate routing/atomic cycles; unrelated-chain preservation and rollback bounds; actual
ranking reversal and incumbent improvements; Fit's battery failure; shape accommodation and
exhaustion; slice ownership; complete retention under three tight caps and late validation;
reference turns; invalid policies; ID/array invariance; rejected-cycle isolation; and immutable
success/stopped CLI artifacts with standalone verification.

`node --test docs/benchmarks/constructive-*/*.test.mjs
docs/benchmarks/user-layout-reconstruction/*.test.mjs docs/benchmarks/chain-prototype/*.test.mjs`
passes 346 tests. `cargo check --workspace` passes; `cargo test --workspace --quiet` passes 524
with two existing ignored tests. No Rust/dependency change was made. Native browser checks pass
24 cases at 1400/736/360 widths in dark/light themes: focus, zoom, pan, fit and overflow.

The slice was compared with the accepted planner design and AGENTS policy before commit.
Real growth is connected, routed and independently checked; temporary body packing is isolated;
complete replacements validate and improve the score. M3's single-transport/component-tree
restrictions remain. No global proof, CP oracle, custom propagator or production cutover was added.

Next: matched inventories/boundaries, one declared total-work allowance per algorithm, common
scoring, completion/work curves, separated proposal/ranking/replay effects and a final research
recommendation. Bounded real continuation is useful here; this greedy fit ranking is not
generally better. Preserve both findings in that final audit.
