# Procedural physical layout generator

One call samples blocks, places them, selects compatible ports, builds a directed
graph, routes every connection, places catalog bridges and independently validates
the complete result. Game geometry and capacities come from external catalogs.
There are no factory templates or embedded facility/recipe constants.

## Run

From the repository root:

```sh
cargo build --release -p aic-layout-gen
target/release/aic-layout-gen generate \
  --config crates/aic-layout-gen/examples/default.json \
  --catalog-dir data/game/normalized \
  --seed 1000 --count 16 --workers 4 \
  --output artifacts/my-procedural-layouts
target/release/aic-layout-gen validate \
  --layout artifacts/my-procedural-layouts/layout-00000.json \
  --catalog-dir artifacts/my-procedural-layouts/catalogs
```

Open the output's `index.html` for the gallery. Individual pages offer belt, pipe
and port visibility controls, route isolation by clicking, and hover descriptions.
Orange lines are belts, blue lines are pipes; outlined squares mark catalog bridges.

`config --output PATH` writes an editable default configuration. Output files and
generation directories must be new. Exit status is 0 when every requested seed
completes, 2 when some seeds exhaust their limits, and 1 for command/input failures.
Structured diagnostics identify the code, severity, stage, message and entity.
An exhausted seed is not a proof that no layout exists.

Each batch stores one `layout-NNNNN.json` per success, SVG and HTML previews,
`case-NNNNN.json` metadata or failure details, the configuration, a `report.json`,
and one snapshot of the three external catalog files. The report contains every
seed and attempt, including failures. Seed identity is `base_seed + slot` with u64
wrapping; changing the worker count preserves physical output bytes. Timing fields
are observational and are not deterministic. Keep actual output bytes for retained
datasets; seed replay across future algorithm versions is not promised.

## Configuration

All fields are explicit; unknown fields and inconsistent targets are rejected.
The seven files in `examples/` supply bounded starting points:

| Profile | Block range | Purpose |
| --- | --- | --- |
| `default` | 12–20 | Mixed transport, branching and merging |
| `small` | 8–12 | Fast small examples |
| `large` | 32–48 | Larger construction stress; strong acceptance bias under these budgets |
| `compact` | 12–20 | Minimum inter-block gaps |
| `spacious` | 12–20 | Larger inter-block gaps |
| `no-bridges` | 8–12 | Forbid same-layer crossings; belt/pipe layer overlaps remain legal |
| `mixed-dag` | 16–24 | Acyclic graph with at least four belt and three pipe links |

`edges_per_block` controls sampled directed edge count. `max_degree` counts all
incoming and outgoing edges. Branch/merge minima count blocks with multiple
outgoing/incoming links, not splitter/converger components. `min_connections`
specifies transport quotas. Empty `facility_ids` uses eligible external facilities;
eligibility requires at least two ports among the requested transports.

`long_link_fraction` is the probability of selecting the distant-only candidate
branch. Distant means at least the 75th percentile of pairwise block-origin
Manhattan distances in that packing. It is not a promised fraction of accepted
edges. The report records the threshold and accepted count.

Attempts resample both geometry and graph targets within the configured ranges.
Consequently successful output is biased toward easier sizes and graphs. For a
fixed-size dataset set both endpoints of `blocks` to that size and inspect failures.
After publication, every connection is a fixed obligation: later rearrangement
must preserve it. This generator has no operation that relaxes a published problem.

`max_cells`, `max_attempts`, `max_pair_trials`, `ports_per_pair`, routing expansion
budgets and bridge limits bound construction work. The CLI also limits count to
4096, workers to 16 and applies a conservative 2 GiB workspace estimate. These
are implementation guards, not game limits or an operating-system RSS cap.
Files read by the CLI and published physical layouts are limited to 16 MiB.

## Library and shared format

```rust,ignore
let loaded = aic_layout_gen::io::load_catalogs(catalog_directory)?;
let generator = aic_layout_gen::Generator::new(&loaded.catalogs, config)?;
let generated = generator.generate(seed)?;
// generated.layout already passed aic_layout::validation::validate.
```

The retained generator borrows validated catalogs. Workers share these immutable
inputs, retain typed route state during construction and publish through a bounded
queue. Catalogs are read, validated and hashed once per command, not per edge.

`aic-layout` owns `aic-physical-layout-v1`: `problem` contains block/facility
identities and connection endpoint identities, commodity labels, transports and
exact rates. `solution` contains poses, selected port IDs, complete paths and
bridge placements. Array ordering is not identity. `catalog_sha256` binds the
layout to the exact external files; the CLI hashes each filename and file bytes
with their little-endian u64 lengths, in facilities/transports/components order.
Independent validation checks coverage, body overlap, ports, rates, adjacency,
layers and declared bridge use. Metrics are derived from actual geometry.

`aic-routing` owns the extracted native A* kernel and reachability preflight.
Existing native factory editing and generation use this same implementation.
Existing `aic-data` catalog/rate exports reference the common types directly.

The physical schema currently describes facility blocks and pairwise routes,
including crossings. It does not encode external anchors, splitter/converger
placements, recipe balance or recipe execution. Synthetic commodity labels are
opaque. Physically valid samples are not automatically admitted by the current
recipe-bound PPO environment, nor are they optimal layouts. Pretraining integration
requires its separate data/admission and evaluation design.

See the [design](../../docs/designs/2026-09-15.00-procedural-layout-generation.md)
and [measured results](../../docs/reports/2026-09-15-procedural-layout-generation.md).
