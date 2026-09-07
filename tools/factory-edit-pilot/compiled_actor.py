"""Instrument one compiled actor without changing parameter ownership or sampling."""

import os
from pathlib import Path
import time

import torch


class CompiledActor:
    def __init__(self, model, cache_directory):
        self.cache_directory = str(Path(cache_directory).resolve())
        self.cache_files_before = sum(p.is_file() for p in Path(self.cache_directory).rglob("*"))
        Path(self.cache_directory).mkdir(parents=True, exist_ok=True)
        os.environ["TORCHINDUCTOR_CACHE_DIR"] = self.cache_directory
        self.events = []
        self.phase = "setup"
        self.pending = None
        self.closed = False
        # These runtime-owned hooks are deliberately isolated from the model/runner.
        try:
            self.handler = torch._dynamo.callback_handler
            for name in ("register_start_callback", "register_end_callback", "remove_start_callback", "remove_end_callback"):
                if not callable(getattr(self.handler, name, None)):
                    raise AttributeError(name)
        except AttributeError as error:
            raise RuntimeError("actor.compile_hooks_unavailable: deployed PyTorch requires compilation callbacks") from error
        self.handler.register_start_callback(self.on_start)
        self.handler.register_end_callback(self.on_end)
        self.cache_counters_before = dict(torch._dynamo.utils.counters["inductor"])
        try:
            self.forward = torch.compile(model, dynamic=True, fullgraph=True)
        except BaseException:
            self.close()
            raise

    def on_start(self, args):
        self.pending = time.perf_counter(), self.phase

    def on_end(self, args):
        started, phase = self.pending
        self.events.append({"code": "actor.compile_event", "severity": "info", "stage": "actor.compile",
                            "entity": str(args.compile_id), "trigger": str(args.callback_trigger),
                            "phase": phase, "seconds": time.perf_counter()-started,
                            "message": "Compiler invocation completed; this may include a disk-cache load"})
        self.pending = None

    def __call__(self, data):
        return self.forward(data)

    def report(self):
        return {"enabled": True, "backend": "inductor", "dynamic": True, "fullgraph": True,
                "cache_directory": self.cache_directory, "cache_files_before": self.cache_files_before,
                "graph_cache_counters": {key: torch._dynamo.utils.counters["inductor"].get(key, 0)-self.cache_counters_before.get(key, 0)
                                         for key in ("fxgraph_cache_hit", "fxgraph_cache_miss", "fxgraph_cache_bypass")},
                "events": list(self.events),
                "event_seconds_are_nested": True,
                "cache_environment": {k: os.environ.get(k) for k in ("TORCHINDUCTOR_FX_GRAPH_CACHE",
                                      "TORCHINDUCTOR_FORCE_DISABLE_CACHES", "TRITON_CACHE_DIR")}}

    def close(self):
        if not self.closed:
            self.handler.remove_start_callback(self.on_start)
            self.handler.remove_end_callback(self.on_end)
            self.closed = True
