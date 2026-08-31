# Phase 3 Prior-Terminal Pair-Value Portfolio

## Purpose

Determine whether the Phase 3 five-second cliff is the unresolved port-value disjunction of the two
same-network `item-xiranite-powder` demand terminals isolated by the complete terminal-subset
experiment.

## Inputs

The experiment retains the selected Phase 3 diagnostic state:

- exact used dimensions `16 x 16`;
- fixed preceding-facility placements;
- the selected introduced-facility coordinate, rotation, and complete port assignment;
- sparse endpoint-support channeling; and
- a five-second search budget for every pair-value case.

Stable preceding-facility bit 2 selects the final-target facility. Stable terminal bits 2 and 3
select its two `item-xiranite-powder` belt-demand terminals. Their compatible port domains are read
from the cumulative exact model rather than hardcoded.

## Exact Partition

Let the complete compatible port domains of the selected terminals be `D2` and `D3`. The portfolio
executes every pair in `D2 x D3`, including pairs that select the same physical port. Each case fixes
only those two port-choice variables.

If `F` is the feasible set of the selected diagnostic state, then:

```text
F = union over (p2, p3) in D2 x D3 of F[p2, p3]
```

The external partition therefore preserves every legal solution. It is not a heuristic port
assignment or a production fallback.

## Outputs

The structured report records:

- both terminal IDs and their complete compatible port domains;
- every executed port pair;
- outcome, construction time, search time, first-incumbent time, model scale, and native search
  counters;
- a complete layout or structured failure report for every case; and
- aggregate feasible, infeasible, unknown, and invalid-witness counts.

The CLI emits JSON, an HTML summary, and one standalone layout HTML file per pair automatically.

## Interpretation

- If most pairs finish quickly, the hidden high-level pair disjunction is the current cliff and an
  exact pair portfolio is a promising orchestration primitive.
- If most pairs still time out, fixed pair values merely expose a deeper endpoint-to-routing cliff.
- A validated witness proves that this restricted placement state can extend to Phase 3.
- Complete infeasibility across every pair proves only that the selected placement state cannot
  extend. It does not prove global Phase 3 infeasibility.

## Invariants

- Every compatible pair is executed exactly once.
- No pair is removed by a hand-written compatibility rule.
- All other preceding ports remain solver decisions.
- Routing, flow, topology, capacity, items, occupancy, and logistics components remain solver
  decisions.
- Diagnostic restrictions do not become production solver constraints.
