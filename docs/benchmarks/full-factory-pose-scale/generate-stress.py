"""Rebuild bounded full-factory vocabulary stress witnesses from the crossing fixture.

These witnesses measure implementation scale. They are not training samples.
"""

import argparse
import copy
import json
from pathlib import Path


ROOT = Path(__file__).resolve().parents[3]
SOURCE = ROOT / "docs/benchmarks/constructive-bound-interconnect/fixtures/crossing-witness.json"


def shifted(value, dy):
    if isinstance(value, dict):
        value = {key: shifted(item, dy) for key, item in value.items()}
        if isinstance(value.get("x"), int) and isinstance(value.get("y"), int):
            value["y"] += dy
        return value
    if isinstance(value, list):
        return [shifted(item, dy) for item in value]
    return value


def replicated(source, replicas, spacing):
    result = {
        "schema_version": source["schema_version"],
        "placements": [],
        "utilizations": {},
        "transport_networks": [],
        "logistics_components": [],
    }
    for index in range(replicas):
        suffix = f":replica-{index:03d}"
        dy = index * spacing
        for placement in source["placements"]:
            placement = shifted(copy.deepcopy(placement), dy)
            placement["instance"] += suffix
            result["placements"].append(placement)
        for instance, utilization in source["utilizations"].items():
            result["utilizations"][instance + suffix] = utilization
        for network in source["transport_networks"]:
            network = shifted(copy.deepcopy(network), dy)
            network["id"] += suffix
            network["requirement_ids"] = [value + suffix for value in network["requirement_ids"]]
            network["component_ids"] = [value + suffix for value in network["component_ids"]]
            for terminal in network["terminals"]:
                terminal["id"] += suffix
                terminal["node"] += suffix
                if "instance" in terminal["endpoint"]:
                    terminal["endpoint"]["instance"] += suffix
                if "node" in terminal["endpoint"]:
                    terminal["endpoint"]["node"] += suffix
            result["transport_networks"].append(network)
        for component in source["logistics_components"]:
            component = shifted(copy.deepcopy(component), dy)
            component["id"] += suffix
            result["logistics_components"].append(component)
    # A physical witness has one network per commodity. Independent replicas
    # remain disconnected subgraphs inside each merged commodity network.
    networks = {}
    for network in result["transport_networks"]:
        key = (network["transport"], network["item"])
        if key not in networks:
            networks[key] = network
        else:
            for field in ("requirement_ids", "cells", "segments", "terminals", "component_ids"):
                networks[key][field].extend(network[field])
    result["transport_networks"] = list(networks.values())
    return result


def tall(source, offset):
    result = shifted(copy.deepcopy(source), offset)
    network = result["transport_networks"][0]
    first = network["segments"][0]
    start = first["from"]
    end = first["to"]
    assert start == {"x": 5, "y": offset + 2} and end == {"x": 6, "y": offset + 2}
    path = ([{"x": 5, "y": y} for y in range(offset + 2, -1, -1)]
            + [{"x": 6, "y": y} for y in range(0, offset + 3)])
    network["cells"].extend(path[1:-1])
    network["segments"] = [
        {"from": a, "to": b, "rate": first["rate"]}
        for a, b in zip(path, path[1:])
    ] + network["segments"][1:]
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--kind", choices=("replicated", "tall"), required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--replicas", type=int, default=51)
    parser.add_argument("--spacing", type=int, default=120)
    parser.add_argument("--offset", type=int, default=6000)
    args = parser.parse_args()
    source = json.loads(SOURCE.read_text())
    if args.kind == "replicated":
        if not 1 <= args.replicas <= 51 or args.spacing < 22:
            parser.error("replicas must be 1..51 and spacing at least 22")
        result = replicated(source, args.replicas, args.spacing)
    else:
        if args.offset < 0:
            parser.error("offset must be nonnegative")
        result = tall(source, args.offset)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, separators=(",", ":")))


if __name__ == "__main__":
    main()
