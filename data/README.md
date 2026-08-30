# External Data

This directory is reserved for runtime-loaded facility, item, and recipe data.

No facility, item, recipe, or transport-balance data should be compiled into application code.

Current example files:

- `examples/recipes.valid.json`: minimal valid recipe book.
- `examples/recipes.graph.json`: recipe graph and throughput example data.
- `examples/throughput.request.json`: throughput request example.
- `examples/throughput.cutoff.request.json`: throughput request that stops expansion at an intermediate input.
- `examples/facilities.valid.json`: footprint, allowed-rotation, and directional-port facility catalog example.
- `examples/items.valid.json`: fixed belt-or-pipe transport kind for every item.
- `examples/placement.request.json`: facility placement system-bound constraints example.

Repository-managed game data:

- `game/source/`: versioned extracted source-table snapshot with provenance and hashes.
- `game/normalized/`: generated runtime catalogs consumed by the CLI and solver.
