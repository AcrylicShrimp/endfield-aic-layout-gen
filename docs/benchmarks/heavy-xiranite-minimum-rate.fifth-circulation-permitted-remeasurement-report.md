# Heavy Xiranite Minimum-Rate Circulation-Permitted Remeasurement Report 5

## Question

After removing hard route-acyclicity proof from the faithful exact formulation, does the first
cumulative SCC phase produce a first incumbent within five seconds, and what is the next measured
first-incumbent blocker?

This is the fifth interactive research checkpoint. It remeasures the same bound series used by
Report 3, connects the result to the feature ablations in Report 4, and stops before selecting or
implementing another formulation change.

## Controlled Method

- Workload: `heavy-xiranite-minimum-rate`
- Target: `item-xiranite-enr-powder`, quantity 1 per 10,000 ms
- Cumulative phase: 0, always one facility, three commodity networks, eight terminals, and four
  route requirements
- Square request ceilings: 12, 16, 20, 24, 32, 40, and 50
- Formulation: `joint-lexicographic-layout-v5`
- Solver: Pumpkin 0.5, release build
- Search budget: 5,000 ms per case
- Repetitions: one per case
- Search: Pumpkin's unchanged default search
- Heuristic fallback: none

Version 5 differs from the measured version 4 formulation only in the relevant routing semantic:
it does not create route-order variables and does not post conditional topological-order
constraints. Positive selected-arc flow, flow conservation, exact terminal supply and demand, line
capacity, branch topology, bridges, collision, used geometry, and the complete objective remain.
Net-zero circulation is therefore legal but cannot manufacture or consume material.

Each case has a static report, exact-model JSON report, self-contained HTML failure view, and
external wall-time/memory record under
`heavy-xiranite-circulation-permitted-bound-sensitivity/`. Normalized data is in
`heavy-xiranite-circulation-permitted-bound-sensitivity.summary.json`.

## Remeasurement Results

All seven cases again reached the five-second limit without a complete incumbent. The result is
`unknown`, not infeasible. No objective value or bound exists because Pumpkin did not complete its
initial satisfaction search.

| Ceiling | Variables | Constraints | Incidences | Build | Search | Wall | Peak RSS | First incumbent |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 12 by 12 | 26,481 | 95,068 | 323,383 | 138 ms | 5,001 ms | 5.68 s | 97.2 MiB | none |
| 16 by 16 | 49,633 | 186,268 | 656,287 | 245 ms | 5,000 ms | 5.27 s | 152.8 MiB | none |
| 20 by 20 | 80,177 | 308,828 | 1,109,415 | 411 ms | 5,001 ms | 5.44 s | 248.0 MiB | none |
| 24 by 24 | 118,113 | 462,748 | 1,682,767 | 634 ms | 5,006 ms | 5.69 s | 389.9 MiB | none |
| 32 by 32 | 216,161 | 864,668 | 3,190,143 | 1,258 ms | 5,004 ms | 6.35 s | 647.2 MiB | none |
| 40 by 40 | 343,777 | 1,392,028 | 5,178,415 | 2,161 ms | 5,006 ms | 7.31 s | 1,044.7 MiB | none |
| 50 by 50 | 544,877 | 2,227,628 | 8,340,015 | 3,942 ms | 5,041 ms | 9.23 s | 1,727.3 MiB | none |

One repetition is enough to establish deterministic structural counts. Timing and memory remain
single-run observations and should not be interpreted as stable performance estimates.

## Version 4 to Version 5 Delta

The exact structural reduction is one route-order variable per network and grid cell plus one
three-term ordering constraint per network and directed grid arc:

```text
removed variables   = 3 N^2
removed constraints = 12 N (N - 1)
removed terms        = 36 N (N - 1)
removed incidences   = 36 N (N - 1)
```

| Ceiling | Variables removed | Constraints removed | Incidences removed | v4 incumbent | v5 incumbent |
| --- | ---: | ---: | ---: | ---: | ---: |
| 12 by 12 | 432 | 1,584 | 4,752 | none | none |
| 16 by 16 | 768 | 2,880 | 8,640 | none | none |
| 20 by 20 | 1,200 | 4,560 | 13,680 | none | none |
| 24 by 24 | 1,728 | 6,624 | 19,872 | none | none |
| 32 by 32 | 3,072 | 11,904 | 35,712 | none | none |
| 40 by 40 | 4,800 | 18,720 | 56,160 | none | none |
| 50 by 50 | 7,500 | 29,400 | 88,200 | none | none |

The new measured square-bound identities are:

```text
variables   = 231 N^2 -   680 N +  1,377
constraints = 980 N^2 - 4,640 N +  9,628
terms       = 3,885 N^2 - 23,122 N + 48,575
incidences  = 3,757 N^2 - 21,970 N + 46,015
```

These identities describe this phase, data revision, formulation, and square-bound series only.
They are not game-wide laws.

## Current Model Breakdown

The route-order family is absent. The remaining variable families are:

| Variable family | Count for side `N` | 12 by 12 | 50 by 50 |
| --- | ---: | ---: | ---: |
| Placement origins and rotations | `4(N - 4)^2` | 256 | 8,464 |
| Directional facility endpoints | `64(N - 4)(N - 5)` | 3,584 | 132,480 |
| Route cells | `3N^2` | 432 | 7,500 |
| Directed route arcs | `12N(N - 1)` | 1,584 | 29,400 |
| Flow on directed arcs | `12N(N - 1)` | 1,584 | 29,400 |
| Terminal presence | `24N^2` | 3,456 | 60,000 |
| Directional route arms | `24N^2` | 3,456 | 60,000 |
| Branch components | `24N^2` | 3,456 | 60,000 |
| Bridges, rotations, and crossing owners | `16N^2` | 2,304 | 40,000 |
| Objective auxiliaries | `48N(N - 1) + 33` | 6,369 | 117,633 |

The largest posted constraint families show that removing acyclicity did not make the remaining
network model small:

| Constraint family | 12 by 12 | 50 by 50 | Primary role |
| --- | ---: | ---: | --- |
| Bridge crossing | 14,656 | 394,960 | Legal crossing ownership and flow |
| Branch topology | 20,304 | 352,500 | Directional branch/component consistency |
| Route-cell activation | 10,768 | 331,260 | Link arcs, terminals, components, and occupied cells |
| Terminal presence | 10,624 | 324,960 | Tie selected endpoints to network cells |
| Turn definition | 16,368 | 312,996 | Third objective and directional geometry |
| Used geometry | 7,722 | 234,202 | Footprint and physical-tile accounting |
| Route arm | 3,456 | 60,000 | Derive directional cell connectivity |
| Line capacity | 3,292 | 59,380 | Shared transport capacity |
| Arc activation | 3,168 | 58,800 | Tie selected arcs to positive flow |

This table is a size breakdown, not a causal ranking. Large families can propagate well, while a
smaller weak or symmetric family can dominate search.

## First-Incumbent Breakdown

Report 4 already isolated the satisfaction boundary before the semantic cutover. Reinterpreted
under the now-approved circulation semantics, its decisive rows are:

| Cumulative or relaxed model | Variables | Constraints | Incidences | First solution |
| --- | ---: | ---: | ---: | ---: |
| Placement only | 256 | 145 | 6,656 | satisfiable, 0 ms |
| Placement + all endpoint choices | 3,840 | 1,173 | 14,848 | satisfiable, 1 ms |
| Above + first network, no acyclicity | 10,224 | 24,765 | 103,040 | satisfiable, 2,695-3,044 ms |
| Above + first network, no acyclicity/branch/bridge | 7,344 | 12,301 | 44,256 | satisfiable, 289-307 ms |
| Above + first two networks, no acyclicity | 15,168 | 53,921 | 205,248 | `unknown`, 5,000 ms |
| Above + first two networks, no acyclicity/branch/bridge | 10,848 | 27,905 | 89,344 | `unknown`, 5,001 ms |
| Above + all three networks, no acyclicity | 20,112 | 70,969 | 255,040 | `unknown`, 5,001 ms |
| Complete v5 model including objectives | 26,481 | 95,068 | 323,383 | `unknown`, 5,001 ms |

The first network is a two-terminal belt network for `item-xiranite-enr-powder`. Under the current
semantics it can produce a complete relaxed-core solution within the budget even with branch and
bridge/collision rules retained. The five-second boundary returns when the second physical
commodity network is added. It remains even when branch topology and bridge/collision modeling are
both removed from the diagnostic model.

## Next Measured Blocker

The next blocker is therefore not placement, endpoint selection, the objective block, or the
removed acyclicity proof. The narrowest currently supported statement is:

> Adding the second commodity network to the 12 by 12 phase-zero satisfaction model prevents
> Pumpkin's default search from finding a first assignment within five seconds, even in the tested
> core with acyclicity, branch topology, bridges, and transport collision absent.

This is a boundary, not yet a root cause. The previous diagnostic added networks in fixed order, so
the evidence does not distinguish between:

1. the second network being intrinsically harder because it has four terminals;
2. the interaction between two belt networks through shared facility placement, endpoint choices,
   cell occupancy, or capacity;
3. a specific remaining flow, terminal-presence, arm, or route-cell activation encoding becoming
   weak only when the second network is present.

The exact next experiment is to solve each of the three phase-zero networks alone and each pair,
then ablate only the remaining shared coupling families around the first failing pair. That matrix
can identify whether the next target is one network's terminal structure or cross-network
interaction without yet changing solver semantics.

## Supported Conclusions

1. Removing hard acyclicity proof substantially shrinks the exact model but does not produce a
   first incumbent for the complete phase-zero model within five seconds at any measured bound.
2. The 12 by 12 failure is not explained by loose-canvas growth; it persists at the smallest
   measured request ceiling.
3. The previous single-network result confirms that circulation-permitted routing itself can cross
   the five-second boundary for one two-terminal network.
4. The next observed barrier is the transition from one to two commodity networks, not an
   optimization stage: the solver still has no first feasible assignment.
5. Current evidence does not justify removing or restricting any remaining exact decision. The
   next checkpoint must isolate network identity and pairwise interaction before proposing a fix.
