# Phase 3 External Boundary-Cell Exact Partition

## Purpose

The accepted four-side partition proves the west subset of fixed `16 x 16` residual tuple case 6
infeasible while north, east, and south remain `Unknown`. This experiment refines the lowest-index
unresolved side into one exact child per surviving boundary key.

## Parent Control

Reproduce the accepted external boundary-side report and require every interpretation gate to pass.
Select the lowest case index whose combined outcome is `Unknown`. Fail closed unless the selected
side has at least two keys and the parent combined outcome remains `Unknown`.

For the accepted Heavy Xiranite report this selects north with eleven keys.

## Exact Partition

Create one child for every key in the selected side's sorted key vector. Each child constructs the
existing selected external boundary selector from the singleton sparse domain `[key]`. The children
are non-empty, pairwise disjoint, and their union is exactly the selected side domain.

The parent-fixed four facility placements/rotations and fifteen facility ports remain fixed. No
route cell, route arc, item, flow, topology, component, or other external terminal is fixed. The
portfolio therefore preserves every legal solution in the selected side and excludes nothing from
the other unresolved sides, which remain represented by their parent cases.

## Execution

Run children in ascending key order. Each child receives:

1. one uninstrumented authoritative feasibility-only solve with a 5,000 ms budget;
2. one separate root-observation solve with a 5,000 ms budget.

Search counters and runtime claims come only from the authoritative solve. The observation solve
may contribute a witness or proof to the symmetric logical outcome but not to cutoff performance.

## Required Assertions

Interpretation is blocked unless:

- the side parent is unblocked and unresolved;
- the selected side is the lowest-index unresolved side;
- singleton domains are non-empty, disjoint, sorted, and exactly cover the selected side keys;
- each selected build certificate's declared domain, unary table projection, routing options,
  restriction metadata, and lower/upper bounds equal `[key]`;
- all non-selected external certificates match the accepted parent contract;
- all child root values for the selected terminal equal `[key]` when root observation is available;
- all four facilities and fifteen facility ports satisfy their fixed contracts;
- child formulations and normalized model metrics satisfy the controlled model contract;
- no invalid witness or witness/proof conflict appears.

## Evidence Aggregation

- Any validated child witness proves the selected side, tuple case 6, and its parent feasible.
- All children proven infeasible close the selected side only.
- A mix of `ProvenInfeasible` and `Unknown` preserves exactly the unknown keys as the next proof
  region.
- No child result changes the status of the other unresolved sides.

## Report

Emit JSON and self-contained HTML with parent provenance, selected side/key set, every child outcome,
model scale, root restriction, construction/search/first-incumbent time, decisions, backtracks,
conflicts, learned clauses, solver propagations, aggregate proof status, and links to all solve/root
wireframes.

## Stop Condition

Commit and report the singleton portfolio before changing dimensions or routing semantics.

- A witness becomes the next visual and semantic validation target.
- A partially closed side continues only on its unresolved singleton keys; no further geometric
  partition exists for proven singleton cells.
- If all selected cells remain `Unknown`, run exact fixed-height widths `13 x 16` through `16 x 16`.
