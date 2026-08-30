#!/usr/bin/env python3
"""Normalize the vendored Endfield factory tables into runtime catalogs."""

from __future__ import annotations

import argparse
import hashlib
import json
from collections import defaultdict
from pathlib import Path
from typing import Any


def load_json(path: Path) -> dict[str, Any]:
    with path.open(encoding="utf-8") as file:
        return json.load(file)


def write_json(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8") as file:
        json.dump(value, file, ensure_ascii=False, indent=2, sort_keys=False)
        file.write("\n")


def stable_id(source_id: str) -> str:
    return source_id.replace("_", "-")


def localized_value(
    text_map: dict[str, str], text_reference: dict[str, Any] | None, fallback: str
) -> tuple[str, str]:
    if text_reference is not None:
        value = text_map.get(str(text_reference["id"]), "").strip()
        if value:
            return value, "official"
    return fallback, "id-fallback"


def file_sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as file:
        for chunk in iter(lambda: file.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def item_transport(item: dict[str, Any]) -> str:
    phase_type = item["phaseType"]
    if phase_type == 1:
        return "belt"
    if phase_type in (2, 4):
        return "pipe"
    raise ValueError(f"unsupported FactoryItemTable phaseType {phase_type} for {item['id']}")


def flatten_amounts(groups: list[dict[str, Any]]) -> list[dict[str, Any]]:
    amounts: dict[str, int] = defaultdict(int)
    for wrapper in groups:
        for amount in wrapper["group"]:
            amounts[stable_id(amount["id"])] += amount["count"]
    return [
        {"item": item_id, "quantity": quantity}
        for item_id, quantity in sorted(amounts.items())
    ]


def port_edge(x: int, y: int, width: int, height: int) -> str:
    if y == 0:
        return "north"
    if x == width - 1:
        return "east"
    if y == height - 1:
        return "south"
    if x == 0:
        return "west"
    raise ValueError(f"port ({x}, {y}) is not on footprint {width}x{height}")


def normalized_port(
    raw_port: dict[str, Any], direction: str, width: int, height: int
) -> dict[str, Any]:
    transport = "pipe" if raw_port["isPipe"] else "belt"
    position = raw_port["trans"]["position"]
    x = position["x"]
    y = position["z"]
    return {
        "id": f"{direction}-{transport}-{raw_port['index']}",
        "direction": direction,
        "transport": transport,
        "position": {"x": x, "y": y},
        "edge": port_edge(x, y, width, height),
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()

    source = args.source
    table_source = source / "table-cfg"
    localization_source = source / "i18n" / "ko-KR"
    source_manifest = load_json(source / "manifest.json")
    blueprint_limits = load_json(table_source / "FacBlueprintConst.json")
    buildings = load_json(table_source / "FactoryBuildingTable.json")
    items = load_json(table_source / "FactoryItemTable.json")
    crafts = load_json(table_source / "FactoryMachineCraftTable.json")
    crafters = load_json(table_source / "FactoryMachineCrafterTable.json")
    belts = load_json(table_source / "FactoryGridBeltTable.json")
    pipes = load_json(table_source / "FactoryLiquidPipeTable.json")
    item_display_records = load_json(table_source / "ItemTable.json")
    item_text = load_json(localization_source / "ItemTable.json")
    building_text = load_json(localization_source / "FactoryBuildingTable.json")
    mode_text = load_json(localization_source / "FactoryMachineCraftModeTable.json")
    craft_text = load_json(localization_source / "FactoryMachineCraftTable.json")
    modes = load_json(table_source / "FactoryMachineCraftModeTable.json")

    if len(belts) != 1 or len(pipes) != 1:
        raise ValueError("expected exactly one belt and one pipe transport definition")
    belt = next(iter(belts.values()))["beltData"]
    pipe = next(iter(pipes.values()))["pipeData"]

    transports_by_item = {
        item_id: item_transport(item) for item_id, item in items.items()
    }

    group_mode: dict[tuple[str, str], str] = {}
    for machine_id, crafter in crafters.items():
        for mode in crafter["modeMap"]:
            key = (machine_id, mode["groupName"])
            if key in group_mode:
                raise ValueError(f"duplicate machine/group mode mapping: {key}")
            group_mode[key] = mode["modeName"]

    mode_recipes: dict[tuple[str, str], list[dict[str, Any]]] = defaultdict(list)
    normalized_recipes = []
    produced_items = set()
    consumed_items = set()

    for source_recipe_id, craft in sorted(crafts.items()):
        machine_id = craft["machineId"]
        group_id = craft["formulaGroupId"]
        mode = group_mode.get((machine_id, group_id))
        if mode is None:
            raise ValueError(
                f"recipe {source_recipe_id} has no unique mode for {machine_id}/{group_id}"
            )

        mode_recipes[(machine_id, mode)].append(craft)
        inputs = flatten_amounts(craft["ingredients"])
        outputs = flatten_amounts(craft["outcomes"])
        consumed_items.update(amount["item"] for amount in inputs)
        produced_items.update(amount["item"] for amount in outputs)

        normalized_recipes.append(
            {
                "id": stable_id(source_recipe_id),
                "facility": f"{stable_id(machine_id)}-mode-{stable_id(mode)}",
                "inputs": inputs,
                "outputs": outputs,
                "duration_ms": craft["progressRound"] * 1000,
            }
        )

    normalized_facilities = []
    for (machine_id, mode), mode_crafts in sorted(mode_recipes.items()):
        building = buildings[machine_id]
        width = building["range"]["width"]
        height = building["range"]["depth"]
        input_transports = {
            transports_by_item[amount["id"]]
            for craft in mode_crafts
            for wrapper in craft["ingredients"]
            for amount in wrapper["group"]
        }
        output_transports = {
            transports_by_item[amount["id"]]
            for craft in mode_crafts
            for wrapper in craft["outcomes"]
            for amount in wrapper["group"]
        }

        ports = [
            normalized_port(port, "input", width, height)
            for port in building["inputPorts"]
            if ("pipe" if port["isPipe"] else "belt") in input_transports
        ]
        ports.extend(
            normalized_port(port, "output", width, height)
            for port in building["outputPorts"]
            if ("pipe" if port["isPipe"] else "belt") in output_transports
        )

        for transport in input_transports:
            if not any(
                port["direction"] == "input" and port["transport"] == transport
                for port in ports
            ):
                raise ValueError(f"{machine_id}/{mode} has no {transport} input port")
        for transport in output_transports:
            if not any(
                port["direction"] == "output" and port["transport"] == transport
                for port in ports
            ):
                raise ValueError(f"{machine_id}/{mode} has no {transport} output port")

        normalized_facilities.append(
            {
                "id": f"{stable_id(machine_id)}-mode-{stable_id(mode)}",
                "footprint": {"width": width, "height": height},
                "allowed_rotations": [0, 90, 180, 270],
                "ports": ports,
            }
        )

    normalized_items = [
        {"id": stable_id(item_id), "transport": transports_by_item[item_id]}
        for item_id in sorted(items)
    ]

    localized_items = []
    for item_id in sorted(items):
        normalized_id = stable_id(item_id)
        display_record = item_display_records.get(item_id)
        display_name, display_name_source = localized_value(
            item_text,
            display_record.get("name") if display_record is not None else None,
            normalized_id,
        )
        localized_items.append(
            {
                "id": normalized_id,
                "display_name": display_name,
                "display_name_source": display_name_source,
            }
        )

    localized_modes = []
    for mode_id, mode in sorted(modes.items()):
        normalized_id = stable_id(mode_id)
        display_name, display_name_source = localized_value(
            mode_text, mode.get("machineModeTypeName"), normalized_id
        )
        localized_modes.append(
            {
                "id": normalized_id,
                "display_name": display_name,
                "display_name_source": display_name_source,
            }
        )

    localized_facilities = []
    for machine_id, mode in sorted(mode_recipes):
        facility_id = f"{stable_id(machine_id)}-mode-{stable_id(mode)}"
        facility_name, facility_name_source = localized_value(
            building_text, buildings[machine_id].get("name"), stable_id(machine_id)
        )
        mode_record = modes.get(mode)
        mode_name, mode_name_source = localized_value(
            mode_text,
            mode_record.get("machineModeTypeName") if mode_record is not None else None,
            stable_id(mode),
        )
        localized_facilities.append(
            {
                "id": facility_id,
                "base_facility": stable_id(machine_id),
                "facility_name": facility_name,
                "facility_name_source": facility_name_source,
                "mode": stable_id(mode),
                "mode_name": mode_name,
                "mode_name_source": mode_name_source,
            }
        )

    localized_recipe_descriptions = []
    for source_recipe_id, craft in sorted(crafts.items()):
        recipe_id = stable_id(source_recipe_id)
        description, description_source = localized_value(
            craft_text, craft.get("formulaDesc"), recipe_id
        )
        localized_recipe_descriptions.append(
            {
                "id": recipe_id,
                "description": description,
                "description_source": description_source,
            }
        )

    write_json(
        args.output / "items.json",
        {"schema_version": 1, "items": normalized_items},
    )
    write_json(
        args.output / "transports.json",
        {
            "schema_version": 1,
            "transports": [
                {
                    "kind": "belt",
                    "capacity": {
                        "quantity": 1,
                        "duration_ms": belt["msPerRound"],
                    },
                },
                {
                    "kind": "pipe",
                    "capacity": {
                        "quantity": pipe["volume"],
                        "duration_ms": pipe["msPerRound"],
                    },
                },
            ],
        },
    )
    write_json(
        args.output / "facilities.json",
        {"schema_version": 3, "facilities": normalized_facilities},
    )
    write_json(
        args.output / "recipes.json",
        {
            "schema_version": 1,
            "external_items": sorted(consumed_items - produced_items),
            "recipes": normalized_recipes,
        },
    )
    write_json(
        args.output / "localization.ko-KR.json",
        {
            "schema_version": 1,
            "locale": "ko-KR",
            "items": localized_items,
            "facilities": localized_facilities,
            "modes": localized_modes,
            "recipe_descriptions": localized_recipe_descriptions,
        },
    )
    write_json(
        args.output / "blueprint-limits.json",
        {
            "schema_version": 1,
            "max_width": blueprint_limits["BluePrintXLenMax"],
            "max_height": blueprint_limits["BluePrintZLenMax"],
            "max_nodes": blueprint_limits["BlueprintNodeCountLimit"],
        },
    )

    generated_files = [
        "blueprint-limits.json",
        "facilities.json",
        "items.json",
        "localization.ko-KR.json",
        "recipes.json",
        "transports.json",
    ]
    write_json(
        args.output / "manifest.json",
        {
            "schema_version": 1,
            "source_upstream_commit": source_manifest["upstream_commit"],
            "generator": "tools/normalize_game_data.py",
            "counts": {
                "blueprint_limits": 1,
                "items": len(normalized_items),
                "transports": 2,
                "facilities": len(normalized_facilities),
                "recipes": len(normalized_recipes),
                "external_items": len(consumed_items - produced_items),
                "localized_items": len(localized_items),
                "localized_facilities": len(localized_facilities),
                "localized_modes": len(localized_modes),
                "localized_recipe_descriptions": len(localized_recipe_descriptions),
            },
            "files": [
                {
                    "path": file_name,
                    "sha256": file_sha256(args.output / file_name),
                }
                for file_name in generated_files
            ],
        },
    )


if __name__ == "__main__":
    main()
