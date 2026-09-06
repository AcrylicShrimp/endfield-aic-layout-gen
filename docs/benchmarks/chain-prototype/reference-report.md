# Milestone 0: rate-correct internal references

Status: complete. The constructor, generated chain candidates, rollback, complete external
factories and matched-budget comparisons remain future milestones. The full goal is active.

## Result

Three screenshot-derived references pass the independent release-mode Rust witness probe,
and every remaining recipe margin matches its declared external obligation exactly.

| Reference | Facilities | Assumed target/min | Bounds / area | Visible belt cells | Pending margins |
| --- | --- | --- | --- | --- | --- |
| Medium battery 01 | 8 | 6 | 10x15 / 150 | 32 | Two iron-ore inputs, three originium-ore inputs, battery export |
| Citron powder 03 | 4 | 60 | 10x12 / 120 | 7 | Powder export only |
| Enriched iron 01 | 9 | 30 | 16x13 / 208 | 19 | Two iron-ore inputs, enriched-powder export |

These are internal references, not generated layouts, complete factory witnesses, observed
steady-state operation, or optimality results. External routes are not fabricated. Both seed
cycles also require bootstrap inventory, which this steady-state geometry test does not model.
The battery geometry was previously confirmed by the user. The two new coordinate transcriptions
are manually inferred and checked against the screenshots, not yet user-confirmed.

The citron seed collector produces 1 seed/s and sends 1/2 seed/s to each full-utilization planter.
One planter feeds the seed collector; the other feeds the grinder. The pending powder export
is 1/s and will require two belt-capacity lanes when external geometry is built.

For enriched iron, both furnace/grinder iron chains and the thickener operate at utilization 1.
The seed collector, both observed planters and plant grinder operate at utilization 1/3.
Each seed branch carries 1/6 per second; the plant grinder supplies 1/2 powder/s. This preserves
the observed nine facilities without inventing extra demand. A rate plan with one planter
instead of two is a different inventory and must not be silently substituted into a layout
quality comparison. Power is excluded; the original battery's historical annotation remains
outside its witness and score.

## What changed and became simpler

The diagnostic transcription contract moved directly to schema 2. Every facility has an
explicit utilization, and every route names its item and rate. The old global-utilization and
implicit full-source-output paths are removed. The neutral geometry witness remains schema 1,
so the Rust validator and production interfaces are unchanged. The original battery witness
is byte-for-byte identical to its pre-migration version.

A second acceptance check now compares independently derived pending margins with the reference's
declared external obligations. A legal partial geometry with a missing internal seed route no
longer passes reference acceptance. A negative CLI test verifies this distinction even when
the geometry validator itself succeeds. The runner preserves a structured rejection report.

Reference pages now identify the recipe-specific assumptions and explicitly separate covered
geometry, reference-margin validation, and full-factory status. The unrelated four-times-rate
demonstration control is removed from these pages. Short display labels avoid overlap without
changing stable facility identities, coordinates, ports, routes or the witness.

## Reproduction

From the repository root, build the independent probe and run the tests:

```sh
cargo build --release -p aic-cli --bin aic-witness-probe
node --test docs/benchmarks/user-layout-reconstruction/*.test.mjs
cargo check --workspace
cargo test --workspace --quiet
```

Run each reference with its actual source screenshot and a fresh output directory:

```sh
node docs/benchmarks/user-layout-reconstruction/run.mjs \
  --case docs/benchmarks/user-layout-reconstruction/citron-powder-03.json \
  --source-image /absolute/path/to/codex-clipboard-b4bd3050-5bac-4dba-9ab8-53070eb9f939.png \
  --recipes data/game/normalized/recipes.json \
  --facilities data/game/normalized/facilities.json \
  --items data/game/normalized/items.json \
  --transports data/game/normalized/transports.json \
  --components data/game/normalized/logistics-components.json \
  --validator target/release/aic-witness-probe \
  --output-directory target/citron-reference-replay
```

For the other cases replace the case, source image and output directory. Battery 01 additionally
uses `--buildings data/game/source/table-cfg/FactoryBuildingTable.json` for its historical power
annotation. The other two cases have no annotations and require no building table.

Final artifacts for this run:

- `target/chain-reference-v2-medium-battery-01-verified/`
- `target/chain-reference-v2-citron-powder-03-verified/`
- `target/chain-reference-v2-enriched-iron-01-verified/`

Each contains `witness.json`, `validation.json`, `validator-process.json`, `validator.log`,
`detail.json`, `report.json`, and `index.html`. Reports hash the case, screenshot, runtime
catalogs and validator executable. Browser evidence is in the corresponding `-verified-browser/`
directories. Initial renders without the final short labels are not the final UI artifacts.

## Verification

All 17 reference/geometry tests pass. They include all three independent positive checks,
byte-identical battery regression, exact split rates, mixed utilization, schema migration
rejection, invalid rational inputs, overallocated flow, body overlap, missing internal routes,
and a CLI failure with valid geometry but incorrect declared margins. No integration test is
silently skipped when the validator executable is absent.

The broader command `node --test docs/benchmarks/constructive-*/*.test.mjs
docs/benchmarks/user-layout-reconstruction/*.test.mjs` also passes all 292 discovered tests.

Workspace check and tests pass: 524 tests passed, two ignored. No Rust files were edited.
Native browser focus, zoom, pan, Fit, and overflow checks pass in 18 cases across the three
pages, using 1400/736/360-pixel viewports and light/dark themes. The final citron and enriched-iron
full renders were visually inspected against the source screenshots. The battery's earlier
schema-2 render was inspected and its final browser checks pass; its witness is unchanged.

## Provenance

SHA-256 witness identities:

- Battery: `5c313c90670156cba95fcc0e6fe3954b43532c2dd370fb7a06550ca9b7b39480`.
- Citron: `9d52f291e2f51943e771897a6314f0b755f8bc764d011c37cc0e1cfaeb8c4c61`.
- Enriched iron: `9fc59f7b6f3488b1cce71abdad32b32607d581fb95d03b909b0dcb57371f6ae2`.

SHA-256 source screenshot identities:

- Battery: `149c197ba0e0d200cc729afe934538ca4f87abfdeab4871f1b9815f02ef22abf`.
- Citron: `d927a93559a057fdb3cce185ecb2f0404f6b21bce6526f247af8b71ca8c266cc`.
- Enriched iron: `4da64fc6a2f6ee80fc3b7a6b16c0db6d4106730f2f016093e6c4cfbaebbb93ef`.

Final schema-2 case identities:

- Battery: `66dc6a9cf56fe9eafcc5ef7abf2502f3fbd696273b2588daab34606417cd2f3b`.
- Citron: `eed550dff0ae4b6a1adfec1f779512a277d91ff62d695312ebc95d5d70ee78fa`.
- Enriched iron: `22d8a9f5020f50df725b9f8aaf34ba068a6916359a51a81235cdd0e973c10df8`.

## Next action and policy audit

Milestone 1 must create straight/bent chain candidates from geometry-free graph inputs, not
reuse these reference poses as hints. Define the exact supported chain DTO and shared work
budget first. Route and validate each newly added facility before growing again. Begin with
a small catalog-backed chain and show the resulting decision trace and actual geometry.

The [accepted planner design](../../designs/2026-09-02.00-constructive-layout-planner.md) and
`AGENTS.md` still govern connected transactional growth, independent validation, no fixed
production W/H limits, pipe-first routing, and area/cells/turns scoring. This slice adds no
production behavior, global exact search, custom propagator or routing fallback. The pending
breadth experiment and pre-existing shared-view edits are preserved but not included in this
work unit. No additional user example is needed to start milestone 1.
