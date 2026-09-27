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

To measure generator yield and spatial occupancy without retaining full layouts:

```sh
target/release/aic-layout-gen survey \
  --config crates/aic-layout-gen/examples/survey.json \
  --catalog-dir data/game/normalized --seed 10000 \
  --output artifacts/layout-generation-survey
```

The survey gives every configured facility-count bucket the same number of raw
construction slots. Each slot fixes its exact facility count, topology, packing
profile and long-connection preference, then performs one attempt. It writes one
compact row for every slot plus facility-bucket and topology/packing summaries.
Occupancy is measured by independent layout validation. Reported quantiles select
the value at `floor((n - 1) * p / 100)` in sorted order; min/max expose rare
extremes. Interrupted and not-started slots remain explicit and make the command
exit with status 2. A terminal generation failure is measured evidence, so it
does not make a completed survey command fail.

The survey report uses `aic-layout-generation-survey-report-v3`. `completed`
means that the raw slot was executed; a `generation-search-incomplete` row is
executed but does not resolve whether the requested layout can be built. An
interrupted slot has `completed=false`. Each row retains aggregate
`routing_statuses` and `attempt_statuses` from its attempts, while the bucket
and topology/packing `failure_reasons` count final diagnostic codes. This keeps
an earlier budget stop visible even if the last attempt ended differently.

Open the output's `index.html` for the gallery. Individual pages offer belt, pipe
and port visibility controls, route isolation by clicking, and hover descriptions.
Orange lines are belts, blue lines are pipes; outlined squares mark catalog bridges.

`config --output PATH` writes an editable default configuration. Output files and
generation directories must be new. Exit status is 0 when every requested seed
completes, 2 when some seeds exhaust their limits, and 1 for command/input failures.
Structured diagnostics identify the code, severity, stage, message and entity.
`generation-search-incomplete` means route work, pair trials or another
resource limit prevented a conclusive construction attempt.
`generation-exhausted` means the configured attempts ended after only fully
evaluated alternatives. Neither code proves global infeasibility.

Each batch stores one `layout-NNNNN.json` per success, SVG and HTML previews,
`case-NNNNN.json` metadata or failure details, the configuration, a `report.json`,
and one snapshot of the three external catalog files. The report contains every
seed and attempt, including failures. Seed identity is `base_seed + slot` with u64
wrapping; changing the worker count preserves physical output bytes. Timing fields
are observational and are not deterministic. Keep actual output bytes for retained
datasets; seed replay across future algorithm versions is not promised.

## Configuration

The public `aic-layout-generator-v2` configuration contains only semantic choices.
Unknown fields and inconsistent requests are rejected. The seven files in
`examples/` supply bounded starting points:

| Profile | Block range | Purpose |
| --- | --- | --- |
| `default` | 12–20 | Mixed transport, branching and merging |
| `small` | 8–12 | Fast small examples |
| `large` | 32–48 | Larger scale-aware construction |
| `compact` | 12–20 | Compact packing profile |
| `spacious` | 12–20 | Spacious packing profile |
| `no-bridges` | 8–12 | Forbid same-layer crossings; belt/pipe layer overlaps remain legal |
| `mixed-dag` | 16–24 | Acyclic graph with at least four belt and three pipe links |

`topology_profiles` selects scale-independent graph shapes. `tree` requests
`n-1` connections, `light-cycles` adds `ceil(n/4)`, and `branching` adds
`ceil(n/2)` plus scale-aware branch and merge requirements. The generator first
samples this semantic request and keeps it fixed across production retries.
`minimum_connections` specifies explicit transport quotas. Empty `facility_ids`
uses eligible external facilities; eligibility requires at least two ports among
the requested transports.

`long_connection_preference` is the probability of selecting the distant-only
candidate branch at each pair trial. Distant means at least the 75th percentile
of pairwise block-origin Manhattan distances in that packing. Ordinary trials
select from the other three quartiles. This is a construction preference rather
than a promised fraction of accepted edges because route feasibility affects
which trials succeed. Each attempt records the threshold and accepted count.

`packing_profiles` contains `compact`, `balanced` and `spacious`. These select
versioned internal spacing policies; final spatial occupancy remains a measured
result. `effort` selects `fast`, `standard` or `thorough`. The versioned
`scale-policy-v4` derives retry count, pair trials, routing work, margin and canvas
guard from the facility and target-connection counts. Every case records the
resolved values. Callers do not scale internal work limits themselves.

After publication, every connection is a fixed obligation: later rearrangement
must preserve it. The CLI limits count to 4096, workers to 16 and applies a
conservative 2 GiB workspace estimate over all configured semantic profiles.
Use `--workspace-budget-gib` to admit a larger finite aggregate estimate.
Files read by the CLI and published physical layouts are limited to 16 MiB.

## Library and shared format

```rust,ignore
let loaded = aic_layout_gen::io::load_catalogs(catalog_directory)?;
let generator = aic_layout_gen::IntentGenerator::new(&loaded.catalogs, config)?;
let result = generator.generate(seed)?;
// result.generated.layout already passed independent physical validation.
// result.resolved records every mechanism value derived by scale-policy-v4.
```

The retained semantic generator borrows validated catalogs. The low-level
`MechanismConfig` remains available for established internal workspace consumers,
but it is not the standalone CLI contract. Workers share immutable catalogs,
retain typed route state during construction and publish through a bounded queue.
Catalogs are read, validated and hashed once per command, not per edge.

`aic-layout` owns `aic-physical-layout-v3`: `problem` contains block/facility
identities, boundary terminals, and connection endpoint identities, commodity
labels, transports and exact rates. `solution` contains facility and terminal
poses, optional facility port IDs, complete paths, catalog junctions and bridge
placements with separately owned horizontal and vertical channels. Compatible
same-commodity routes may share a straight transport arm within catalog line
capacity; branches and merges require a matching selected catalog component.
Array ordering is not identity. `catalog_sha256` binds the
layout to the exact external files; the CLI hashes each filename and file bytes
with their little-endian u64 lengths, in facilities/transports/components order.
Independent validation checks coverage, body overlap, ports, rates, adjacency,
layers, directed junction flow and bridge-axis ownership. Physical-v2 JSON must
be explicitly migrated; its bridge connection pair and implicit junctions are
not interpreted as physical-v3. Metrics are derived from actual geometry.

`aic-routing` owns the extracted native A* kernel and reachability preflight.
Existing native factory editing and generation use this same implementation.
Existing `aic-data` catalog/rate exports reference the common types directly.

The physical schema currently describes facility blocks, boundary anchors and
pairwise flow routes, including crossings and shared same-commodity cells. It
does not encode explicit splitter/converger component placements, recipe balance
or recipe execution. Synthetic commodity labels are
opaque. Physically valid samples are not automatically admitted by the current
recipe-bound PPO environment, nor are they optimal layouts. Pretraining integration
requires its separate data/admission and evaluation design.

See the [design](../../docs/designs/2026-09-15.00-procedural-layout-generation.md)
and [scale-aware configuration design](../../docs/designs/2026-09-16.01-scale-aware-layout-generator.md).
