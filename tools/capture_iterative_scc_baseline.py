#!/usr/bin/env python3
"""Capture normalized metrics for the known-bad iterative SCC baseline."""

from __future__ import annotations

import argparse
import hashlib
import json
import platform
import subprocess
import time
from pathlib import Path
from typing import Any


REPOSITORY_ROOT = Path(__file__).resolve().parents[1]
BUILD_COMMAND = ["cargo", "build", "--release", "--quiet", "-p", "aic-cli"]
SOLVE_COMMAND = [
    "target/release/aic-cli",
    "--data-dir",
    "data/game/normalized",
    "layouts",
    "solve-contextual",
    "--recipes",
    "data/game/normalized/recipes.json",
    "--source-plan",
    "data/examples/source-plan.game-heavy-xiranite-forge.request.json",
    "--facility-catalog",
    "data/game/normalized/facilities.json",
    "--item-catalog",
    "data/game/normalized/items.json",
    "--transport-catalog",
    "data/game/normalized/transports.json",
    "--logistics-component-catalog",
    "data/game/normalized/logistics-components.json",
    "--placement-request",
    "data/examples/placement.factory-500.request.json",
    "--optimization-config",
    "data/examples/layout.iterative-optimization.config.json",
    "--strategy",
    "iterative-scc",
]
INPUT_PATHS = [
    "data/game/normalized/recipes.json",
    "data/examples/source-plan.game-heavy-xiranite-forge.request.json",
    "data/game/normalized/facilities.json",
    "data/game/normalized/items.json",
    "data/game/normalized/transports.json",
    "data/game/normalized/logistics-components.json",
    "data/examples/placement.factory-500.request.json",
    "data/examples/layout.iterative-optimization.config.json",
]


def file_sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as file:
        for chunk in iter(lambda: file.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def command_output(command: list[str]) -> str:
    return subprocess.run(
        command,
        cwd=REPOSITORY_ROOT,
        check=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    ).stdout.strip()


def route_turn_count(route: dict[str, Any]) -> int:
    cells = route["cells"]
    turns = 0
    for first, middle, last in zip(cells, cells[1:], cells[2:]):
        first_horizontal = first["y"] == middle["y"]
        second_horizontal = middle["y"] == last["y"]
        turns += first_horizontal != second_horizontal
    return turns


def used_geometry_bounds(phase: dict[str, Any]) -> dict[str, int]:
    cells: list[tuple[int, int]] = []
    for placement in phase["placements"]:
        for x in range(placement["x"], placement["x"] + placement["width"]):
            for y in range(placement["y"], placement["y"] + placement["height"]):
                cells.append((x, y))
    for route in phase["routes"]:
        cells.extend((cell["x"], cell["y"]) for cell in route["cells"])
    cells.extend(
        (component["position"]["x"], component["position"]["y"])
        for component in phase["logistics_components"]
    )
    if not cells:
        return {
            "min_x": 0,
            "min_y": 0,
            "max_x": -1,
            "max_y": -1,
            "width": 0,
            "height": 0,
            "area": 0,
        }
    min_x = min(x for x, _ in cells)
    min_y = min(y for _, y in cells)
    max_x = max(x for x, _ in cells)
    max_y = max(y for _, y in cells)
    width = max_x - min_x + 1
    height = max_y - min_y + 1
    return {
        "min_x": min_x,
        "min_y": min_y,
        "max_x": max_x,
        "max_y": max_y,
        "width": width,
        "height": height,
        "area": width * height,
    }


def transport_metrics(routes: list[dict[str, Any]], transport: str) -> dict[str, int]:
    selected = [route for route in routes if route["transport"] == transport]
    unique_cells = {
        (cell["x"], cell["y"])
        for route in selected
        for cell in route["cells"]
    }
    return {
        "route_count": len(selected),
        "logical_route_cells": sum(len(route["cells"]) for route in selected),
        "route_edges": sum(max(0, len(route["cells"]) - 1) for route in selected),
        "unique_route_tiles": len(unique_cells),
    }


def phase_metrics(phase: dict[str, Any]) -> dict[str, Any]:
    routes = phase["routes"]
    reported_bounds = phase["bounds"]
    boundary_endpoints = sum(
        endpoint["kind"] == "boundary"
        for route in routes
        for endpoint in (route["source"], route["target"])
    )
    zero_rate_routes = sum(route["rate"]["numerator"] == 0 for route in routes)
    return {
        "phase_index": phase["index"],
        "introduced_scc_ids": phase["introduced_components"],
        "introduced_facility_count": len(phase["introduced_facilities"]),
        "cumulative_facility_count": phase["cumulative_facility_count"],
        "reported_bounds": {
            "width": reported_bounds["width"],
            "height": reported_bounds["height"],
            "area": reported_bounds["width"] * reported_bounds["height"],
        },
        "independently_measured_used_geometry": used_geometry_bounds(phase),
        "route_count": len(routes),
        "total_logical_route_cells": sum(len(route["cells"]) for route in routes),
        "total_route_edges": sum(max(0, len(route["cells"]) - 1) for route in routes),
        "total_route_turns": sum(route_turn_count(route) for route in routes),
        "belt": transport_metrics(routes, "belt"),
        "pipe": transport_metrics(routes, "pipe"),
        "boundary_endpoint_count": boundary_endpoints,
        "active_zero_flow_route_count": zero_rate_routes,
        "logistics_component_count": len(phase["logistics_components"]),
        "bridge_count": sum(
            component["kind"] == "bridge"
            for component in phase["logistics_components"]
        ),
        "selected_movement_radius": phase["selected_movement_radius"],
    }


def host_description() -> dict[str, str]:
    return {
        "system": platform.system(),
        "release": platform.release(),
        "machine": platform.machine(),
        "rustc": command_output(["rustc", "--version"]),
    }


def input_descriptions() -> list[dict[str, str]]:
    return [
        {
            "path": relative_path,
            "sha256": file_sha256(REPOSITORY_ROOT / relative_path),
        }
        for relative_path in INPUT_PATHS
    ]


def normalize_report(
    report: dict[str, Any], elapsed_seconds: float, source_commit: str
) -> dict[str, Any]:
    layout = report["layout"]
    if not report["success"] or not layout["success"]:
        raise ValueError("baseline capture requires a successful layout report")
    phases = [phase_metrics(phase) for phase in layout["phases"]]
    if not phases:
        raise ValueError("iterative SCC baseline requires phase history")
    placement_request = json.loads(
        (REPOSITORY_ROOT / "data/examples/placement.factory-500.request.json").read_text(
            encoding="utf-8"
        )
    )
    return {
        "schema_version": 1,
        "benchmark_id": "heavy-xiranite-forge-minimum-rate",
        "strategy": "iterative-scc",
        "baseline_status": "known-bad-diagnostic-only",
        "source_commit": source_commit,
        "host": host_description(),
        "build_command": BUILD_COMMAND,
        "solve_command": SOLVE_COMMAND,
        "input_files": input_descriptions(),
        "search_domain": {
            "max_width": placement_request["max_width"],
            "max_height": placement_request["max_height"],
        },
        "solve_time_seconds": round(elapsed_seconds, 6),
        "layout_status": layout["status"],
        "phase_count": len(phases),
        "phases": phases,
        "final_phase_index": phases[-1]["phase_index"],
        "known_bad_observations": [
            "Phase zero routes every external connection to a fixed perimeter cell.",
            "The current result is a first-feasible diagnostic baseline, not an expected layout.",
            "Later slices must replace this artifact rather than preserve its coordinates or scores.",
        ],
    }


def capture_report(skip_build: bool) -> tuple[dict[str, Any], float]:
    if not skip_build:
        subprocess.run(BUILD_COMMAND, cwd=REPOSITORY_ROOT, check=True)
    started = time.perf_counter()
    process = subprocess.run(
        SOLVE_COMMAND,
        cwd=REPOSITORY_ROOT,
        check=True,
        stdout=subprocess.PIPE,
        text=True,
    )
    elapsed_seconds = time.perf_counter() - started
    return json.loads(process.stdout), elapsed_seconds


def write_json(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8") as file:
        json.dump(value, file, ensure_ascii=False, indent=2, sort_keys=False)
        file.write("\n")


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Capture normalized metrics for the known-bad iterative SCC baseline."
    )
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument(
        "--report",
        type=Path,
        help="Normalize an existing contextual layout report instead of running the solver.",
    )
    parser.add_argument(
        "--elapsed-seconds",
        type=float,
        help="Measured solve duration for --report; required when --report is used.",
    )
    parser.add_argument(
        "--source-commit",
        help="Source revision to record; defaults to the current Git HEAD.",
    )
    parser.add_argument(
        "--skip-build",
        action="store_true",
        help="Run the release binary without rebuilding it first.",
    )
    args = parser.parse_args()

    if args.report is not None:
        if args.elapsed_seconds is None:
            parser.error("--elapsed-seconds is required with --report")
        report = json.loads(args.report.read_text(encoding="utf-8"))
        elapsed_seconds = args.elapsed_seconds
    else:
        if args.elapsed_seconds is not None:
            parser.error("--elapsed-seconds may only be used with --report")
        report, elapsed_seconds = capture_report(args.skip_build)

    source_commit = args.source_commit or command_output(["git", "rev-parse", "HEAD"])
    write_json(
        args.output,
        normalize_report(report, elapsed_seconds, source_commit),
    )


if __name__ == "__main__":
    main()
