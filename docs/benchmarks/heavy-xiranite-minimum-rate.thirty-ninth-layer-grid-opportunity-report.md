# Heavy Xiranite Minimum-Rate Layer Grid Opportunity Report

## Result

A diagnostic-only analyzer that understands each belt or pipe layer as one two-dimensional grid
finds a material number of exact routing deductions that the current local formulation leaves
unresolved.

Before the controlled cumulative Phase 2 `12x12` first witness, the analyzer observes:

- 66 distinct unique-support route arcs;
- 194 distinct unresolved activation or item predicates;
- 6,103 unresolved-predicate observations across search states; and
- a maximum backward unique-support chain of 19 arcs.

The smallest proposed pruning slice—only the arc directly entering a selected demand—contains:

- nine distinct terminal-support arcs;
- 27 distinct unresolved predicates; and
- 1,293 unresolved-predicate observations.

This passes the experiment's decision rule. The next slice should implement only terminal-adjacent
unique-support forcing with a sound reachability reason, then measure whether its search-tree
reduction repays its runtime cost.

## Controlled Search Evidence

The analyzer is passive. It posts no predicate and adds no decision variable. Its case preserves the
accepted broad-lazy search tree and witness exactly:

| Metric | Broad lazy | Broad lazy + analyzer |
| --- | ---: | ---: |
| First witness | 3,919 ms | 4,169 ms |
| Branch decisions | 43,226 | 43,226 |
| Backtracks | 11,511 | 11,511 |
| Conflicts | 11,505 | 11,505 |
| Learned clauses | 11,505 | 11,505 |
| Connectivity executions | 144,775 | 144,775 |
| Grid-analyzer executions | 0 | 32,716 |
| Objective | `144/99/45/12/15` | `144/99/45/12/15` |

The analyzer adds about 250 ms in this run. An active propagator must eliminate enough native search
to recover that cost. The analyzer's implementation is intentionally simple and is not a proposed
production hot path.

## Exact Opportunity Definition

For one selected demand, start from all still-possible material supplies and traverse the possible
directed grid. If the demand is reachable and exactly one possible incoming arc from a reachable
cell can enter it, every legal completion connecting that selected demand must use that arc with the
required material on both directional arms.

Following the same rule backward identifies longer mandatory chains, but this report does not
authorize forcing those chains. The first active experiment is restricted to the terminal-adjacent
arc so its explanation remains small and auditable.

## Artifact

- `heavy-xiranite-phase2-layer-grid-terminal-opportunity-5s/summary.html`
- `heavy-xiranite-phase2-layer-grid-terminal-opportunity-5s/summary.json`
