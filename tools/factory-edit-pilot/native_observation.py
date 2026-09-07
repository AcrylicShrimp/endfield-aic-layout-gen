"""Explicit experimental native observation geometry; no training backend selection."""

import importlib.machinery
import importlib.util
from pathlib import Path
import time

import numpy as np

from commands import CandidateSet
from environment import Environment, Mechanical, SpatialGuard


def load_native(path):
    path = Path(path).resolve(strict=True)
    loader = importlib.machinery.ExtensionFileLoader("aic_pilot_native", str(path))
    spec = importlib.util.spec_from_file_location("aic_pilot_native", path, loader=loader)
    module = importlib.util.module_from_spec(spec)
    loader.exec_module(module)
    return module


def native_environment(module):
    class NativeEnvironment(Environment):
        def __init__(self, catalog, config):
            super().__init__(catalog, config)
            terminals = [(tid, self.nids.index(nid), self.fids.index(endpoint["facility"])
                          if "facility" in endpoint else None, role == "source")
                         for tid, (nid, role, endpoint) in self.terminal_metadata.items()]
            sizes = [(catalog.facility_type(fid)["footprint"]["width"],
                      catalog.facility_type(fid)["footprint"]["height"]) for fid in self.fids]
            self.native_encoder = module.GeometryEncoder(self.fids, self.nids, sizes, terminals,
                                                        self.port_offsets, self.object_template, self.scale)

        def observe(self, candidates):
            started = time.perf_counter()
            if not isinstance(candidates, CandidateSet) or candidates.state is not self.draft or candidates.revision != self.revision:
                raise Mechanical("observation.stale_candidates: regenerate candidates for the current draft")
            try:
                raster, objects, (x0, y0) = self.native_encoder.encode(
                    self.draft, self.base, self.config["halo"], self.config["raster_cell_guard"])
            except ValueError as error:
                if str(error).startswith("observation.resource_guard:"):
                    raise SpatialGuard(str(error)) from error
                raise
            height, width = raster.shape[1:]
            # Keep NumPy arange/division semantics, including large world coordinates.
            raster[-3] = np.arange(x0, x0+width, dtype=np.float32)[None, :]/self.scale
            raster[-2] = np.arange(y0, y0+height, dtype=np.float32)[:, None]/self.scale
            raster[-1] = 1
            features, absolute_points = zip(*(self.encode_candidate(command, self.command_context(command))
                                             for command in candidates.commands))
            points = np.asarray(absolute_points, dtype=np.int64)-np.array([x0, y0, x0, y0], dtype=np.int64)
            stats = [(self.config["horizon"]-self.steps)/self.config["horizon"],
                     self.best_score[0]/self.initial_score[0], self.base_score[0]/self.initial_score[0],
                     self.best_score[1]/self.initial_score[0], self.base_score[1]/self.initial_score[0],
                     self.best_score[2]/self.initial_score[0], self.base_score[2]/self.initial_score[0],
                     sum(b is None for b in self.draft["bindings"].values())/len(self.tids)]
            observation = {"raster": raster, "objects": objects, "stats": np.asarray(stats, dtype=np.float32),
                           "candidates": np.asarray(features, dtype=np.float32), "points": points}
            cache = self.encode_candidate.cache_info()
            self.counters["encoding_cache_hits"] = cache.hits
            self.counters["encoding_cache_misses"] = cache.misses
            self.counters["encoding_cache_entries"] = cache.currsize
            self.seconds["observation"] += time.perf_counter()-started
            return observation

    return NativeEnvironment
