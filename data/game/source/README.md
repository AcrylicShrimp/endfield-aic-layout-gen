# Game Data Source Snapshot

This directory contains a repository-managed snapshot of extracted official game tables used to build the normalized runtime catalogs under `data/game/normalized/`.

## Provenance

- Game data repository: <https://github.com/rmxlinux/EndfieldData>
- Upstream commit: `83be4dcdeab69f93799febff674af6f19b8972be`
- Upstream timestamp: `2026-08-26T23:08:22+08:00`
- Upstream update label: `Update 8/26`
- Snapshot date: `2026-08-30`
- Extracted localization data API: <https://endfield-assets.fffdan.com>
- Localization retrieval date: `2026-08-30`

The upstream repository describes itself as an extracted game-data repository and does not include a license file in this snapshot. These data files are not covered by this project's `MIT OR Apache-2.0` source-code license. Review redistribution requirements before publishing a derived distribution.

Snapshot JSON is content-equivalent to the pinned upstream blobs. Repository text files include a terminal newline; manifest checksums identify the committed local bytes.

The `i18n/ko-KR/` dictionaries are table-scoped projections of the extracted official Korean text map. They were retrieved through the listed data API and are pinned by committed checksums. They are not fetched at runtime or during normalization.

## Included Tables

- `FacBlueprintConst.json`: blueprint width, height, and node-count limits.
- `FactoryBuildingTable.json`: footprint and fixed directional port records.
- `FactoryBuildingRendererTemplateTable.json`: machine and mode renderer variants.
- `FactoryGridBeltTable.json`: belt timing and line definitions.
- `FactoryGridConnecterTable.json`: belt connector port directions.
- `FactoryGridRouterTable.json`: belt splitter and converger geometry and port directions.
- `FactoryItemTable.json`: factory-item phase metadata.
- `FactoryLiquidConnectorTable.json`: pipe connector geometry and port directions.
- `FactoryLiquidPipeTable.json`: pipe timing, volume, and line definitions.
- `FactoryLiquidRouterTable.json`: pipe splitter and converger geometry and port directions.
- `FactoryMachineCraftModeCoverTable.json`: mode coverage relationships.
- `FactoryMachineCraftModeTable.json`: known machine mode types.
- `FactoryMachineCraftTable.json`: recipe inputs, outputs, machine IDs, and durations.
- `FactoryMachineCrafterTable.json`: machine-to-mode and recipe-group mappings.
- `ItemTable.json`: item display-name text IDs.
- `i18n/ko-KR/FactoryBuildingTable.json`: Korean facility text values.
- `i18n/ko-KR/FactoryMachineCraftModeTable.json`: Korean machine-mode text values.
- `i18n/ko-KR/FactoryMachineCraftTable.json`: Korean recipe formula descriptions.
- `i18n/ko-KR/ItemTable.json`: Korean item text values.

## Update Policy

Do not edit source tables by hand. When game data changes, replace the relevant files from one identified upstream commit, update `manifest.json`, regenerate normalized catalogs, validate them through the CLI, and commit the source and normalized changes together.

Mode-dependent machines are flattened only in normalized data. Source table IDs and structures remain unchanged in this snapshot.
