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
  --report-output /tmp/aic-production.json
```

The constructor is incremental work in progress. This real example currently returns a validated
partial layout and a nonzero exit code; it does not yet complete the factory. Read `status`,
`internal_complete`, `complete`, and the unresolved requirements in the JSON report. External I/O
realization remains unfinished even when a controlled graph's internal construction completes.
There are no W/H inputs. Budgets bound operations, not game geometry.

The `construct-production` command and request/report schema 1 replace the removed
`auto-assemble-process-modules` command and its automatic assembly API. There is no compatibility
alias or exact-solver fallback. Manually selected module commands remain diagnostic experiments.
See [the cutover experiment](docs/benchmarks/constructive-completion-first/report.md).

## Legacy Placement Diagnostic

The older placement-only command below does not construct or validate a complete routed factory:

```bash
cargo run -p aic-cli -- layouts place-facilities \
  --recipes data/examples/recipes.graph.json \
  --throughput-request data/examples/throughput.request.json \
  --facility-catalog data/examples/facilities.valid.json \
  --placement-request data/examples/placement.request.json
```
