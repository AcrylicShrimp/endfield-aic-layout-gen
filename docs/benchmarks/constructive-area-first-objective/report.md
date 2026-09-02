# Constructive Area-First Objective Report

## Result

The measured bottleneck was caused by an incorrect score priority. The planner treated preservation
of unused port options as more important than compact geometry. Unused catalog ports are optional,
so composition schema version 3 now minimizes:

1. used bounding-box area;
2. occupied belt and pipe tiles;
3. route turns;
4. blocked optional boundary-port choices as a final tie-breaker.

This makes every valid area incumbent safe to share across composition workers and substantially
strengthens the existing pre-A* area lower-bound rejection.

## Heavy Xiranite Six-Step Comparison

Both rows use release builds and the same six-step automatic-module-discovery request. The selected
partial factory may differ because the objective itself was corrected.

| Metric | Previous port-first score | Area-first score |
| --- | ---: | ---: |
| Wall time | 8.45 s | 0.84 s warm median |
| Facilities | 15 | 16 |
| Routed requirements | 14 | 15 |
| Used bounds | 53x11 | 42x11 |
| Used area | 583 | 462 |
| Occupied belt tiles | 102 | 60 |
| Occupied pipe tiles | 0 | 0 |
| Route turns | not retained | 3 |
| Selected-composition A* searches | 35,468 | 1,832 |
| Unresolved facility requirements | not retained | 9 |
| Unresolved edge boundaries | not retained | 14 |
| Minimum options on one edge boundary | not retained | 1 |

The area decreases by 20.8%, warm wall time decreases by about 10x, and selected-composition A*
searches decrease by 94.8%. A first post-build run took 1.45 seconds; three immediately repeated
runs took 0.85, 0.84, and 0.84 seconds.

## Interpretation

The result validates the score correction and the shared area bound. It does not validate the
current logical-edge boundary model as the final port contract. That model still conservatively
requires every unresolved logical edge to retain a port option. The production graph may merge
same-item flow before a facility port, and catalog ports not selected for a required flow may be
blocked entirely.

The next slice should introduce a capacity-grouped port-demand analysis. For each facility,
direction, item, and transport kind, it must calculate:

```text
required ports = ceil(total item rate / transport line capacity)
```

Distinct items and belt/pipe groups remain separate. This analysis should first be emitted as a
structured report and checked against Heavy Xiranite before it replaces the interim edge-boundary
validity rule.

## Reproduction

```bash
cargo run --release -p aic-cli -- layouts auto-assemble-process-modules \
  --recipes data/game/normalized/recipes.json \
  --source-plan data/examples/source-plan.game-heavy-xiranite-forge.request.json \
  --facility-catalog data/game/normalized/facilities.json \
  --item-catalog data/game/normalized/items.json \
  --transport-catalog data/game/normalized/transports.json \
  --target-instance facility-instance:recipe-occurrence:/target:0 \
  --max-steps 6 \
  --localization-catalog data/game/normalized/localization.ko-KR.json
```

The machine-readable report and self-contained visualization are stored beside this document.
