# Game Data Source Snapshot

This directory contains a repository-managed snapshot of extracted official game tables used to build the normalized runtime catalogs under `data/game/normalized/`.

## Provenance

- Game data repository: <https://github.com/rmxlinux/EndfieldData>
- Upstream commit: `83be4dcdeab69f93799febff674af6f19b8972be`
- Upstream timestamp: `2026-08-26T23:08:22+08:00`
- Upstream update label: `Update 8/26`
- Snapshot date: `2026-08-30`

The upstream repository describes itself as an extracted game-data repository and does not include a license file in this snapshot. These data files are not covered by this project's `MIT OR Apache-2.0` source-code license. Review redistribution requirements before publishing a derived distribution.

## Included Tables

- `FactoryBuildingTable.json`: footprint and fixed directional port records.
- `FactoryBuildingRendererTemplateTable.json`: machine and mode renderer variants.
- `FactoryGridBeltTable.json`: belt timing and line definitions.
- `FactoryItemTable.json`: factory-item phase metadata.
- `FactoryLiquidPipeTable.json`: pipe timing, volume, and line definitions.
- `FactoryMachineCraftModeCoverTable.json`: mode coverage relationships.
- `FactoryMachineCraftModeTable.json`: known machine mode types.
- `FactoryMachineCraftTable.json`: recipe inputs, outputs, machine IDs, and durations.
- `FactoryMachineCrafterTable.json`: machine-to-mode and recipe-group mappings.

## Update Policy

Do not edit source tables by hand. When game data changes, replace the relevant files from one identified upstream commit, update `manifest.json`, regenerate normalized catalogs, validate them through the CLI, and commit the source and normalized changes together.

Mode-dependent machines are flattened only in normalized data. Source table IDs and structures remain unchanged in this snapshot.

