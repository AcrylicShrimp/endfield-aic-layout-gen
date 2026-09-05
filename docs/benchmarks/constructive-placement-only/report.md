# Placement-only review and source-direction incident

Historical pre-fix report and artifacts. The subsequent correction, regenerated placement and
timing measurements are in `../constructive-corrected-ports/report.md`; the current served
`placement-only.html` has been updated. The incident evidence below remains unchanged.

## Visual milestone

The user requested all facilities without any routing, to inspect placement before another routing
experiment. The isolated Node CLI produces Heavy Xiranite's full 59-facility placement at 52 x 92,
zero routes, zero assigned ports, with explicit `factory_validated: false`. Initial placement took
about 24 ms in Node, not a Rust constructor speed comparison. Thirty additions shift rows for
clearance and eighteen logical requirements are not forward in the global axis. The small cycle
also places both facilities. Neither is a game-valid factory claim.

The diagnostic uses breadth-first belt-priority discovery and belt-first orientation, top-aligned
one-cell-gapped rows and obstacle-edge row shifts. Ports do not influence row selection. All
catalog ports are shown by default after user feedback: inward green and outward purple arrows,
solid belt leads and dashed blue pipe leads. Optional graph links are not physical routes. Stage
controls fit the currently visible prefix; selected facilities show stable IDs and port details.
F labels are local to this run, not identical to prior constructive histories.

Reproduce with `node docs/benchmarks/constructive-placement-only/place.mjs --wiring WIRING_JSON
--facilities data/game/normalized/facilities.json --items data/game/normalized/items.json
--target facility-instance:recipe-occurrence:/target:0 --report-output NEW_JSON
--visualization-output NEW_HTML`. Wiring comes from the existing contextual-instance-wiring CLI.
`heavy.json` preserves the frozen placement and input/source hashes. Full temporary outputs are
under `/tmp/aic-placement-only.RbG210/`; the existing tailnet server serves `placement-only.html`.

Seven Node tests pass, including straight 1:1 chains despite offset port IDs, collision-only row
changes, belt-first breadth-first order, cycles, shared consumers, disconnected nodes, rotations,
invalid inputs and safe HTML embedding. Workspace check and 509 Rust tests pass (two existing
research ignores). Browser checks pass for default port visibility, stage fit, facility inspection,
graph overlays and no horizontal overflow at 360/736 pixels in light/dark mode (16 checks).
No production Rust behavior, exact solver, runtime catalog or normalizer is changed in this slice.

## Blocking discovery during visual review

User identified one downward input on F37, `planter-1-mode-normal` (official Korean name in the
localization catalog: Planting Unit). The renderer correctly projects the supplied catalog; the
catalog is wrong. All five original planter belt inputs have source rotation Y = 180 and lie along
the same south edge. The normalizer ignores rotation and uses coordinate priority north, east,
south, west. Thus the southeast corner becomes east rather than south. After the diagnostic's
90-degree rotation, it points south instead of west.

Pinned original source:
https://github.com/rmxlinux/EndfieldData/blob/83be4dcdeab69f93799febff674af6f19b8972be/TableCfg/FactoryBuildingTable.json
(planter record, input ports 0 through 4). Compare `tools/normalize_game_data.py:68`.

`source-port-audit.json` records all 250 current catalog ports, the unambiguous non-corner
direction/rotation mapping, and 20 inconsistent corner ports across 20 of 28 facility/mode entries.
All recorded discrepancies are south-facing source inputs incorrectly normalized east. Source
rotation classes observed here are input 180 -> south, input 90 -> west, output 180 -> north,
output 90 -> east. Do not reuse the logistics-component heading convention without checking it;
factory and logistics source transforms are distinct contracts.

Reproduce the read-only audit with `node docs/benchmarks/constructive-placement-only/audit-ports.mjs
data/game/source/table-cfg/FactoryBuildingTable.json data/game/normalized/facilities.json
BASELINE_PRODUCTION_REPORT NEW_AUDIT_JSON`. No data is corrected by this command.

## Historical impact

- `git blame -L 68,94 tools/normalize_game_data.py` identifies introduction in `5b62859`,
  2026-08-30 16:24:38 +0900, the initial normalized real-game catalog commit.
- `git log --follow -- tools/normalize_game_data.py` shows that function unchanged since introduction.
- `facilities.json` has only that one historical commit. Initial and current SHA-256 both equal
  `db398048ac401024f7bae80c05ae59d4a9fb9a037f6836f33d26c5d08f0bcf96`.
- Therefore real-game joint placement/routing, later SCC exact research and the constructive planner
  all had this incorrect port domain available. This does not apply retroactively to May's logical
  recipe/throughput work or automatically to independent synthetic fixtures.
- The latest preserved complete 59-facility baseline's actual terminal list contains **zero** of
  the affected ports. That witness is not proven physically wrong by this audit. Search-space,
  alternative-port, orientation and failure behavior may still be affected. Do not claim all previous
  work is invalid, nor retain game-level feasibility/optimality/failure conclusions without review.

The generator and independent geometry validator share the same normalized catalog. Internal
consistency tests cannot detect this semantic import error; source-to-catalog regression checks
were missing. The placement-only page remains an inspection artifact of that catalog, not a fixed
version. Stop the planned fixed-placement routing probe until normalization is corrected, catalogs
are regenerated and validated, and representative historical witnesses/searches are rechecked.
Correction of the normalizer is not implemented by this diagnostic-only user question.
