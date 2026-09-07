# Constructive regression inputs

These deliberately selected fixtures replace dependencies on historical benchmark
reports. They are test inputs, not factory results or production fallbacks. Each
file is below 100 KiB. `constructive.mjs` is a test-only loader and never runs a
planner, router, allocation search, or ordering solver.

- `graph.json` contains all 59 facility identities/recipes and all 96 ordered
  requirements with exact rational rates. Its explicit `identities` dictionary
  stores original endpoint IDs once. `Fxx` references are also the original labels;
  `Xxx` references identify external endpoints. Expanding references does not
  shorten, sort, regenerate, or hash any identity. Facility and edge arrays retain
  their original order. Recipe and facility identifiers refer to runtime catalogs.
- `heavy-layout.json` and `gap6-layout.json` store only ordered
  `[identity_reference, x, y, rotation]` rows, the target reference, and, for gap6,
  ordered rigid-block IDs, member references and channel allowances. These fixed
  choices are necessary inputs to geometry regressions, not new placement results.
  Width/height, block envelopes and every port are derived from the runtime
  facility catalog through `rotateFacility`. No game geometry or port catalog is
  duplicated here. Gap1 needs no coordinate variant: its consumers only normalize
  the graph. The heavy and gap6 facility array orders are deliberately different.
- `banks.json` specifies the final ordered 80-edge bank input. `{ "base": ID }`
  reuses an unchanged graph requirement; other entries give the replacement edge
  with dictionary endpoint references. This preserves the full graph after the
  historical attached graph and sixteen allocation deltas, including unrelated
  edges. No seven-facility crop or new boundary interpretation is introduced.
- `order-certificate.json` retains one exact local order witness: the original
  block permutation and twelve corridor lane-to-port assignments. This small
  certificate is essential to test independent checking of a simultaneous valid
  order and rejection of an invalid port without requiring an ordering solver.
  It is scoped to selected independent eastward belt corridors; pipes, returns,
  external connections and global factory validity remain unresolved. Lane
  geometry/rates are recomputed by the existing checker. Solver statistics,
  redundant endpoint IDs and raw search output are omitted.
- `manifest.json` records the original source byte hashes, catalog hashes, and
  hashes of the expanded semantic inputs. Historical paths are provenance only:
  no test opens them. Semantic hashes use UTF-8 `JSON.stringify` after recursively
  sorting object keys, preserving every array order and omitting absent fields.
  They were captured from the original reports before the cutover, not from the
  loader's reconstructed output. Intentional catalog updates require reviewing
  their effect on the fixed geometry and updating this evidence explicitly.
  Geometry fingerprints also include `target_instance`: changing the construction
  target changes placement behavior even when the frozen coordinates are identical.
  The target-inclusive fingerprints were recalculated from the byte-verified
  archived originals at the manifest's origin paths, not reconstructed fixtures.

`loadGraphSnapshot()` supplies the existing `normalizeGraph` input contract;
`loadBankGraph(catalogs)` supplies the final normalized bank graph;
`loadPlacementFixture('heavy' | 'gap6')` projects the frozen placement inputs;
`loadOrderFixture()` supplies the small certificate. Loads are detached objects.
The placement loader checks membership, allowed rotations and facility clearance,
including duplicate members within an individual rigid block,
then supplies required diagnostic DTO fields only in memory. Alignment counts and
pipe relations are derived from the complete graph and projected ports. It does
not call `placeOnly`, `chainBlocks`, or a production constructor.

The chain CLI regression writes the assembled heavy DTO into its temporary test
directory with hashes of the catalog bytes actually loaded. Existing CLI catalog
and geometry checks remain active. Source-report hashes are not reused as hashes
of reconstructed files. The local-order regression invokes its existing model
checker directly, so it does not require a historical serialized-placement hash.

Verification: `node --test docs/benchmarks/fixtures/constructive.test.mjs` checks
the original expanded semantic fingerprints, full graph margins, detached loads
and file sizes. Existing affected constructive tests retain their topology,
ownership, geometry, order, invalid-port and CLI assertions. Tests that invoke a
Rust validator use the existing release binary; fixture assembly never builds it.

Run the affected tests from the repository root:

```sh
node --test \
  docs/benchmarks/fixtures/constructive.test.mjs \
  docs/benchmarks/constructive-block-composition/model.test.mjs \
  docs/benchmarks/constructive-chain-blocks/model.test.mjs \
  docs/benchmarks/constructive-cluster-packing/model.test.mjs \
  docs/benchmarks/constructive-cycle-blocks/model.test.mjs \
  docs/benchmarks/constructive-factory-closure/model.test.mjs \
  docs/benchmarks/constructive-local-reembedding/groups.test.mjs \
  docs/benchmarks/constructive-local-reembedding/model.test.mjs \
  docs/benchmarks/constructive-reciprocal-banks/model.test.mjs \
  docs/benchmarks/constructive-reciprocal-strips/direct.test.mjs \
  docs/benchmarks/constructive-reciprocal-strips/model.test.mjs \
  docs/benchmarks/constructive-scc-groups/project.test.mjs \
  docs/benchmarks/constructive-zero-crossing/model.test.mjs
```
