"""Immutable internal candidates with an unchanged serialized edit boundary."""

from dataclasses import dataclass
from typing import NamedTuple


class Command(NamedTuple):
    op: str
    args: tuple = ()

    def to_dict(self):
        op, args = self
        if op == "SET_PLACEMENTS":
            return {"op": op, "assignments": {fid: list(pose) for fid, pose in args}}
        if op == "UNBIND_TERMINALS":
            return {"op": op, "terminal": args[0]}
        if op == "BIND_TERMINALS":
            tid, binding = args
            value = {"port": binding[1]} if binding[0] == "port" else {"cell": list(binding[1]), "side": binding[2]}
            return {"op": op, "terminal": tid, "binding": value}
        if op == "ERASE_TRANSPORT":
            return {"op": op, "network": args[0], "cells": [list(c) for c in args[1]]}
        if op == "EXTEND":
            nid, anchor, growth, direction, length = args
            return {"op": op, "network": nid, "anchor": list(anchor), "growth": growth,
                    "direction": direction, "length": length}
        if op == "CONNECT":
            return {"op": op, "network": args[0], "source": list(args[1]), "target": list(args[2])}
        if op == "DISCARD_EDIT":
            return {"op": op}
        raise ValueError(f"command.unknown: {op}")


@dataclass(frozen=True, slots=True)
class CandidateSet:
    commands: tuple
    revision: int
    state: dict
    index: tuple

    def to_dicts(self):
        """Materialize the external domain for diagnostics, never for training selection."""
        return [command.to_dict() for command in self.commands]
