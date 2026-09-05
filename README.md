# endfield-aic-layout-gen

CLI tools for generating construction layouts for the Arknights: Endfield Automated Industry Complex.

## Workspace

This repository is a Rust virtual workspace. Crates live under `crates/` and use the `aic-` prefix.

- `crates/aic-cli`: command line entry point
- `crates/aic-data`: external data models, loaders, validators, and graph resolution

Facility and recipe data must stay outside the compiled application and be loaded from external data files at runtime. Facility catalogs include canonical footprints, allowed rotations, and directional belt or pipe ports; placed ports can be projected into world-grid routing endpoints.

## Recipe Commands

Validate a recipe JSON file:

```bash
cargo run -p aic-cli -- recipes validate --file data/examples/recipes.valid.json
```

Resolve the dependency graph for a target item:

```bash
cargo run -p aic-cli -- recipes graph --file data/examples/recipes.graph.json --target originium-casing
```

## Production Construction

Construct connected facilities at their natural footprint, accepting the first valid local growth
instead of optimizing compact modules before the factory is complete:

```bash
cargo run --release -p aic-cli -- layouts construct-production \
  --recipes data/game/normalized/recipes.json \
  --source-plan data/examples/source-plan.game-heavy-xiranite-forge.request.json \
  --facility-catalog data/game/normalized/facilities.json \
  --item-catalog data/game/normalized/items.json \
  --transport-catalog data/game/normalized/transports.json \
  --logistics-component-catalog data/game/normalized/logistics-components.json \
  --target-instance 'facility-instance:recipe-occurrence:/target:0' \
  --max-steps 256 --max-placement-attempts 256 --max-route-searches 4096 \
  --visualization-output /tmp/aic-production.html \
  --report-output /tmp/aic-production.json > /dev/null
```

This real example now completes all 59 facilities, 89 internal requirements and seven external
input/output requirements under the current catalog and validation contract. It returns
`status=complete`, `complete=true`, no unresolved requirements and exit 0. The constructor remains
heuristic: other requests may return a validated partial layout and a nonzero exit code. There are
no W/H inputs. Budgets bound operations, not game geometry. The initial layout is not optimized.

The `construct-production` command uses request schema 1 and report schema 2. It replaces the removed
`auto-assemble-process-modules` command and its automatic assembly API. There is no compatibility
alias or exact-solver fallback. Manually selected module commands remain diagnostic experiments.
Report schema 2 adds step stages and explicitly resolved external requirements. Catalog-backed
bridges preserve independent crossing channels; full success includes a separate original-wiring
identity/rate audit. See [the complete-graph experiment](docs/benchmarks/constructive-complete-graph/report.md)
and [the earlier cutover](docs/benchmarks/constructive-completion-first/report.md).

## Legacy Placement Diagnostic

The older placement-only command below does not construct or validate a complete routed factory:

```bash
cargo run -p aic-cli -- layouts place-facilities \
  --recipes data/examples/recipes.graph.json \
  --throughput-request data/examples/throughput.request.json \
  --facility-catalog data/examples/facilities.valid.json \
  --placement-request data/examples/placement.request.json
```
