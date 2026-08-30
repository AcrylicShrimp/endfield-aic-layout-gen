#!/usr/bin/env python3
"""Normalize the vendored Endfield factory tables into runtime catalogs."""

from __future__ import annotations

import argparse
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
    buildings = load_json(source / "FactoryBuildingTable.json")
    items = load_json(source / "FactoryItemTable.json")
    crafts = load_json(source / "FactoryMachineCraftTable.json")
    crafters = load_json(source / "FactoryMachineCrafterTable.json")

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

    write_json(
        args.output / "items.json",
        {"schema_version": 1, "items": normalized_items},
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


if __name__ == "__main__":
    main()
