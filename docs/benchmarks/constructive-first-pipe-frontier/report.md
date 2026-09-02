# First Constructive Pipe Frontier

## Purpose

This baseline checks the smallest complete constructive growth transaction against the real Heavy Xiranite contextual production graph:

1. select one facility-to-facility pipe requirement;
2. place the target facility;
3. place its upstream supplier;
4. select compatible directional pipe ports;
5. route the selected connection with A*;
6. emit a structured result and the existing interactive wireframe HTML.

This slice deliberately accepts the first feasible transaction. It does not optimize placement candidates, port choices, or the resulting local route. Doing so before later frontier growth could greedily block ports or corridors needed by facilities that have not yet been added.

## Heavy Xiranite Result

- Status: constructed
- Requirement: first internal liquid Xiranite pipe edge
- Rate: `1/2`
- Facilities: 2
- Used bounds: `12x6`
- Pipe tiles: 16
- Route turns: 2
- Seed placements considered: 1
- Supplier placements considered: 61
- Overlapping placements rejected: 60
- Port pairs considered: 1
- A* searches: 1
- A* failures: 0
- Warm release runtime: below the `time(1)` 10 ms display resolution

The long route is expected baseline behavior. A* found the best path for the first selected placement and port pair, while the constructor intentionally did not compare that pair against later feasible candidates. Route quality belongs to bounded growth backtracking and post-construction iterative improvement.

## Verification

```text
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
cargo build --release --workspace
target/release/aic-cli layouts construct-first-pipe-frontier \
  --recipes data/game/normalized/recipes.json \
  --source-plan data/examples/source-plan.game-heavy-xiranite-forge.request.json \
  --facility-catalog data/game/normalized/facilities.json \
  --item-catalog data/game/normalized/items.json \
  --visualization-output docs/benchmarks/constructive-first-pipe-frontier/heavy-xiranite.html \
  --localization-catalog data/game/normalized/localization.ko-KR.json
```

The generated HTML was opened in the in-app browser. It uses the existing wireframe renderer, shows two localized facilities, pipe tiles, directional arrows, labels, and the click inspector.
