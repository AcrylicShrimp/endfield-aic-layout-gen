# Normalized Game Data

These runtime catalogs are deterministically generated from the vendored source snapshot with:

```sh
python3 tools/normalize_game_data.py \
  --source data/game/source/table-cfg \
  --output data/game/normalized
```

Current snapshot contents:

- `items.json`: 538 items with fixed `belt` or `pipe` transport.
- `facilities.json`: 28 recipe-capable machine-mode combinations flattened into dedicated facilities.
- `recipes.json`: 305 machine recipes and 10 raw external input items.

## Normalization Rules

- Source snake-case IDs become stable kebab-case IDs.
- Factory item phase type `1` becomes `belt`.
- Factory item phase types `2` and `4` become `pipe`; this covers liquid and gas logistics carried by pipe ports.
- Every `(machine, mode)` pair referenced by a recipe group becomes `<machine>-mode-<mode>`.
- Every normalized recipe directly references its flattened facility ID.
- Facility footprint width and height come from source `range.width` and `range.depth`.
- Port 2D coordinates come from source position `x` and `z`.
- Mode ports include only fixed input/output transport kinds used by recipes in that mode.
- All four quarter-turn rotations are enabled for recipe-capable facilities.
- Recipe duration is `progressRound * 1000` milliseconds.
- Items consumed but never produced by a machine recipe become recipe-book external items.

The source snapshot remains authoritative. Do not hand-edit generated catalogs; change the normalizer or source snapshot and regenerate them.

