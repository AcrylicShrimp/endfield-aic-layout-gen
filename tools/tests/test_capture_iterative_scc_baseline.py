from __future__ import annotations

import importlib.util
import unittest
from pathlib import Path


SCRIPT_PATH = Path(__file__).resolve().parents[1] / "capture_iterative_scc_baseline.py"
SPEC = importlib.util.spec_from_file_location("capture_iterative_scc_baseline", SCRIPT_PATH)
assert SPEC is not None and SPEC.loader is not None
BASELINE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BASELINE)


class BaselineMetricsTests(unittest.TestCase):
    def test_phase_metrics_distinguish_logical_cells_from_physical_tiles(self) -> None:
        phase = {
            "index": 0,
            "introduced_components": ["component:0000"],
            "introduced_facilities": ["facility:a"],
            "cumulative_facility_count": 1,
            "selected_movement_radius": None,
            "bounds": {"width": 4, "height": 3},
            "placements": [
                {"x": 1, "y": 1, "width": 2, "height": 2}
            ],
            "routes": [
                {
                    "source": {"kind": "boundary"},
                    "target": {"kind": "facility"},
                    "transport": "belt",
                    "rate": {"numerator": 1, "denominator": 1},
                    "cells": [
                        {"x": 0, "y": 1},
                        {"x": 1, "y": 1},
                        {"x": 1, "y": 2},
                    ],
                },
                {
                    "source": {"kind": "facility"},
                    "target": {"kind": "boundary"},
                    "transport": "belt",
                    "rate": {"numerator": 0, "denominator": 1},
                    "cells": [{"x": 1, "y": 2}, {"x": 0, "y": 2}],
                },
                {
                    "source": {"kind": "boundary"},
                    "target": {"kind": "facility"},
                    "transport": "pipe",
                    "rate": {"numerator": 1, "denominator": 1},
                    "cells": [{"x": 0, "y": 1}, {"x": 0, "y": 2}],
                },
            ],
            "logistics_components": [
                {"kind": "bridge", "position": {"x": 3, "y": 2}}
            ],
        }

        metrics = BASELINE.phase_metrics(phase)

        self.assertEqual(metrics["total_logical_route_cells"], 7)
        self.assertEqual(metrics["total_route_edges"], 4)
        self.assertEqual(metrics["total_route_turns"], 1)
        self.assertEqual(metrics["belt"]["unique_route_tiles"], 4)
        self.assertEqual(metrics["pipe"]["unique_route_tiles"], 2)
        self.assertEqual(metrics["boundary_endpoint_count"], 3)
        self.assertEqual(metrics["active_zero_flow_route_count"], 1)
        self.assertEqual(
            metrics["independently_measured_used_geometry"],
            {
                "min_x": 0,
                "min_y": 1,
                "max_x": 3,
                "max_y": 2,
                "width": 4,
                "height": 2,
                "area": 8,
            },
        )


if __name__ == "__main__":
    unittest.main()
