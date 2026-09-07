# Actual heavy-xiranite cyclic production blocks

## Result

The full frozen graph contains 59 facilities and 96 requirements (89 internal, seven actual
external obligations). Runtime recipe-margin checks recover consistent exact utilization for every
facility. Eight productive SCCs are recognized without facility-label or recipe-ID rules. Including
their direct product consumers, these blocks own 35 distinct facilities; 24 facilities remain outside.

All eight blocks have independently valid two-way internal production and return networks. No
external terminal exists in any selected witness. All facility inputs are satisfied. Nine recipe
output terminals remain pending, corresponding to twelve original outgoing graph requirements.
The original 54 within-block pair requirements are represented by their exact per-facility commodity
margins; compatible pair allocations can change. Do not interpret 54 as a count of preserved
point-to-point routes. The other 42 original requirements still need factory-level work.

This is not a complete factory or a single composed layout. Do not add block areas and report the
sum as the whole factory's footprint. Cyclic flows follow the existing steady-state model; startup
seed inventory is not modeled or claimed to be solved.

| Block family | Instances | Facilities per block | Used bounds | Physical tiles | Return perimeter channels |
| --- | ---: | ---: | --- | ---: | ---: |
| Two plant suppliers, one return supplier, one grinder (moss-2) | 6 | 4 | 22 x 14 | 80 | 2 |
| Same topology, different runtime recipe/rates (moss-3) | 1 | 4 | 24 x 14 | 91 | 2 |
| Three plant suppliers, two return suppliers, two grinders | 1 | 7 | 38 x 26 | 324 | 4 |

The first large-cycle geometry expanded six return fragments before the perimeter. Visual inspection
showed six unnecessarily long nested loops. Keeping each source capacity bucket shared through the
return, then splitting near the target bank, reduces those loops to four. With exactly the same
recipes, utilization and ownership, the compact result improves from 42 x 28 / 456 tiles to
38 x 26 / 324 tiles: area decreases 16.0%, physical tiles 28.9%. Both alternatives remain explicit,
independently validated candidates; minimum area, not fewer parts, selects the winner.

## Construction and scope

The graph reader discards historical coordinates, rotations and ports. It checks every incoming
and outgoing commodity margin against the runtime recipe, including the original external edges,
before discovering SCCs. The supported shape is a productive two-commodity belt cycle with
single-input/output SCC recipes and a closed immediate forward-consumer bank. Unsupported SCCs
remain explicit failures and are not dropped from the graph. All eight actual SCCs meet this scope.

The common two-bank engine builds the forward product network. Its temporary external return
network is replaced, not retained: source capacity ports, optional three-way splitting, ordered
nested U paths and target merging form the internal return. The return can split before or after
the perimeter. Untouched downstream product requirements are removed from the temporary external
representation and retained as exact pending full-graph cuts. The independent validator checks the
actual embedded paths. A separate coverage contract rejects fake externals and requires pending
rates to match those cuts, facility by facility, item by item.

Each candidate contains every original member exactly once. Provenance retains original facility
IDs, labels, within-block requirements and outgoing edges. The displayed F labels come from that
mapping; no label is a geometry rule. `compact.mjs` now transforms actual return `port_x` as well
as `port_y`, and moves old perimeter annotations into `pre_compaction_plan` instead of leaving
stale coordinates in the new geometry.

This is isolated JavaScript research using the existing release witness validator, not a Rust
migration, production cutover or global exact fallback. The production design at
`docs/designs/2026-09-02.00-constructive-layout-planner.md` remains unchanged. Eventual integration
must commit connected, validated growth and preserve pending outside obligations transactionally.

## Verification and reviewed fixes

- 32 actual candidates pass independent covered-geometry and exact cycle-coverage checks:
  eight blocks, two return families, raw and compacted variants.
- Eight independent audit expectations pass: unchanged positive control plus broken return segment,
  missing return network, fake external return, undeclared output cut, duplicate/missing member and
  changed utilization. Notably, missing-return and fake-external witnesses can pass covered-geometry
  validation; the new cycle-coverage gate correctly rejects them.
- 34 related Node tests pass, including graph margin/schema rejection, frozen-coordinate/label
  independence, non-duplicated ownership, both geometry families, compaction/port coordinates and
  coverage diagnostics. Unit report fixtures test the coverage contract only; actual Rust controls
  are separate, not mocked acceptance evidence.
- `cargo check --workspace` and `cargo test --workspace` pass: 524 tests and two existing ignores.
  No Rust code or dependencies changed.
- Actual small moss-2, small moss-3 and large cycle renders inspected. Six viewport/theme checks
  per representative page pass. The upper return loops and pending grinder outputs are visible.

An independent read-only review identified two actionable problems, reproduced locally before fixes:
external edges with missing/non-string endpoints were accepted; and semantic coverage rejections
could have empty diagnostics after successful geometry validation. Graph endpoint validation now
rejects those inputs. The boolean coverage helper was replaced with a diagnostic-returning contract;
candidate `coverage.json` and aggregate reports retain stable codes and expected/actual rates.

Improved mutation tests also exposed shared references between witness utilization and its expected
request snapshot. These DTOs are now detached. A utilization-only mutation cannot change its own
acceptance baseline. Coverage also checks missing/duplicate terminal report rows and original
recipe totals. These checks supplement, not replace, independent physical validation.
A final independent read-only recheck confirmed snapshot isolation across all 32 candidates and
found no additional confirmed regression. It reran the 32-test unit subset; the full related
suite including CLI controls contains 34 tests.

Source/catalog/executable/witness hashes and timings are retained in `matrix.json`; negative-control
evidence is in `controls.json`. Generated complete detail/geometry/coverage JSON files remain under
the matrix's recorded output directories. Timing separates graph preparation, inventory I/O and
geometry construction; candidate validators run with four-worker bounded concurrency. It excludes
Rust builds. These are fresh local observations, not CPU-isolated performance guarantees.
The retained run measured 16.88 ms for graph preparation, 2.75 ms for inventory I/O, 89.42 ms
for all sixteen raw family constructions plus their compacted variants, and 738.58 ms through
candidate validation/rendering and selected-page output. This is not timing for a complete factory.

## Next composition gate

The selected output ports are still physically unassigned and their obligations remain pending.
Visual inspection and the review's free-cell reachability check show exits around the blocks, but
this is not proof that all outgoing belts can coexist. Test composition before more local polishing.

The smallest useful next case combines `cycle-0` (F41 product), `cycle-2` (F42 product) and F28:
nine actual facilities. F28 consumes two powder commodities at 1/s and 1/2/s, requiring three belt
capacity lanes under the runtime catalog. F42's remaining 1/2/s output to F29 must remain explicitly
pending; do not duplicate F42 or externalize its missing consumer. Then add `cycle-1` and F29 to
exercise the whole shared-output pair with fourteen facilities. The remaining full graph includes
six downstream processing chains and the side-pipe final-product region. Full completion is pending.

One local quality opportunity remains visible in the smaller moss-3 loop: filling source capacity
buckets first causes a split followed by a merge, although partially filled dedicated ports may
avoid them. This is a future area-first port-partition candidate, not a reason to delay testing
whole-graph composition or to minimize component count independently of area.

```sh
node docs/benchmarks/constructive-cycle-blocks/run.mjs \
  --graph artifacts/benchmarks/constructive-chain-blocks/gap-1-placement.json \
  --recipes data/game/normalized/recipes.json --facilities data/game/normalized/facilities.json \
  --items data/game/normalized/items.json --transports data/game/normalized/transports.json \
  --components data/game/normalized/logistics-components.json \
  --validator target/release/aic-witness-probe --output-directory NEW_RESULT_DIRECTORY
node docs/benchmarks/constructive-cycle-blocks/audit.mjs \
  --baseline NEW_RESULT_DIRECTORY/report.json \
  --recipes data/game/normalized/recipes.json --facilities data/game/normalized/facilities.json \
  --items data/game/normalized/items.json --transports data/game/normalized/transports.json \
  --components data/game/normalized/logistics-components.json \
  --validator target/release/aic-witness-probe --output-directory NEW_CONTROL_DIRECTORY
```
