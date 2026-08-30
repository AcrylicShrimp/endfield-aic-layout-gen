---
name: aic-solver-cliff-diagnosis
description: Diagnose the next performance cliff in the AIC exact layout solver by constructing controlled release-mode experiment matrices, preserving solver semantics, and pausing at an evidence-backed research decision. Use for requests to break down a timeout, memory spike, search-space explosion, or newly exposed solver blocker in this repository.
---

# AIC Solver Cliff Diagnosis

Locate the smallest change that turns a tractable exact model into an intractable one, then identify
which model families grew across that boundary. Do not optimize the implementation or choose the
next architecture in the same diagnostic checkpoint.

## Establish the comparison

1. Read `AGENTS.md`, the current accepted solver design, and the most recent benchmark report.
2. Inspect existing artifacts before adding instrumentation. Reuse stable report fields and CLI
   loaders where practical.
3. State one falsifiable question and write a short experiment contract under `docs/designs/` before
   implementation. Define the controlled formulation, matrix axis, equal per-case resource budget,
   outputs, and stopping point.
4. Keep the latest exact formulation unchanged across cases. A timeout is `unknown`, never
   infeasible.

Choose the narrowest matrix supported by current evidence. Typical axes are bound size, cumulative
SCC phase, clean logical-network subsets, or one precisely defined model feature. Do not vary more
than one independent axis unless the report explicitly separates it.

## Preserve the research subject

- For a composition comparison, rebuild each exact model from the selected logical input. Do not
  leave excluded networks present with routes forced to zero; that retains their ports, domains, and
  auxiliary variables and corrupts the comparison.
- Do not fix placement, ports, or routes in the faithful baseline matrix.
- Do not add corridors, candidate crops, constructive seeds, deterministic placement, or other
  heuristics.
- A constraint-family removal or fixed-decision run may be used only as a separately labelled
  diagnostic ablation after the faithful cliff is established. Mark it `diagnostic_only`; never
  present its geometry, feasibility, or objective as a valid production result.
- Keep production solver behavior unchanged. Research commands and schemas should be separate and
  removable.

## Measure each case

Run the optimized release binary. Give every comparable case its own equal wall-clock budget and
record at least:

- selected logical scope and stable IDs;
- variable counts by kind and domain sizes or log-domain volume;
- constraint counts by family, terms/factor incidences, and placement-routing coupling;
- routing cells, arcs, networks, and endpoint candidates;
- model construction time and solver search time separately;
- first incumbent time, final objective vector, best bound/gap when available;
- termination reason, proof status, and validation status.

Write machine-readable JSON and self-contained HTML for success and failure. Measure RSS in isolated
processes when comparing cases: process peak RSS cannot be reset reliably between cases in one
matrix process. Record the platform/tool and units.

## Interpret and stop

Report the matrix as deltas, not only absolute totals. Identify:

1. the first case that crosses from an incumbent to no incumbent, or the sharpest cost jump if all
   cases have the same outcome;
2. which variable, domain, constraint, and coupling families account for most of that jump;
3. what the evidence rules out; and
4. one next experiment that can distinguish the remaining explanations.

Do not silently continue into that next experiment. Commit the contract and the completed diagnostic
slice with verification results, link the report and artifacts, state whether the worktree is clean,
and pause for the user to review the evidence.
