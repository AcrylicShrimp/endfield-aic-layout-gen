# endfield-aic-layout-gen

CLI tools for generating construction layouts for the Arknights: Endfield Automated Industry Complex.

## Workspace

This repository is a Rust virtual workspace. Crates live under `crates/` and use the `aic-` prefix.

- `crates/aic-cli`: command line entry point
- `crates/aic-data`: external data models, loaders, validators, and graph resolution

Facility and recipe data must stay outside the compiled application and be loaded from external data files at runtime.

## Recipe Commands

Validate a recipe JSON file:

```bash
cargo run -p aic-cli -- recipes validate --file data/examples/recipes.valid.json
```

Resolve the dependency graph for a target item:

```bash
cargo run -p aic-cli -- recipes graph --file data/examples/recipes.graph.json --target originium-casing
```

Generate a feasible facility placement from the full production pipeline:

```bash
cargo run -p aic-cli -- layouts place-facilities \
  --recipes data/examples/recipes.graph.json \
  --throughput-request data/examples/throughput.request.json \
  --facility-catalog data/examples/facilities.valid.json \
  --placement-request data/examples/placement.request.json
```
