# V2 Cumulative SCC Growth Experiment

## Status

Accepted measurement slice. This experiment does not cut the production layout command over from
the current exact model. It measures whether the accepted shared-layer, factored-endpoint v2 model
can grow beyond Heavy Xiranite phase zero before that model becomes authoritative.

## Question

Starting from the proven Heavy Xiranite phase-zero result, what is the first cumulative SCC phase
at which the v2 exact model loses an optimal proof, a complete incumbent, or model construction
within a fixed per-phase budget?

## Solve Contract

For target phase `T`, the runner reconstructs phases `0..=T` in output-first SCC order. Each phase:

1. projects the complete logical wiring onto the cumulative facility set;
2. builds the full legal v2 joint placement, rotation, port-selection, external-connector, and
   internal shared-layer routing model for that projected graph;
3. gives Pumpkin the previous successful phase's common facility placements as non-binding search
   hints;
4. gives newly introduced facilities and every port, connector, route, and component decision its
   complete legal domain;
5. optimizes the normal lexicographic objective; and
6. independently validates every returned complete incumbent.

Placement hints affect branch order only. The runner does not copy route state, fix coordinates,
crop domains, choose ports, select corridors, or fall back to another solver. A failed phase returns
structured evidence and the preceding successful phase history.

The research path validates each cumulative projection independently. It intentionally does not
reject an early phase because the complete production graph cannot fit the same request bounds.
The request bounds remain hard ceilings for each measured phase, not blueprint dimensions or a
project invariant.

## Controlled Matrix

The first matrix uses:

- workload: Heavy Xiranite minimum rate;
- request ceiling: 12 by 12;
- formulation: `joint-shared-transport-layer-external-connectors-v2`;
- release build;
- 5,000 ms search budget independently for every cumulative phase;
- one fresh process for every target phase so peak RSS and process time are attributable;
- JSON and self-contained HTML for success, timeout, invalid input, or infeasibility.

Start at phase zero and increase the target by one. Record the first proof cliff but continue while a
validated incumbent exists. Stop the matrix at the first phase with no incumbent, at a proven hard-
bound infeasibility, or at an invalid model. A hard-bound failure is reported separately from a
search cliff and does not justify treating 12 by 12 as a canonical limit.

## Required Evidence

For every target phase, preserve:

- introduced SCC IDs and facility IDs;
- cumulative facility and logical requirement counts;
- external connector, commodity network, and terminal counts;
- variable, constraint, and term totals plus recorded families;
- model construction time;
- search time and first-incumbent time;
- incumbent count, objective vector, active-stage bounds, proof, and termination;
- placement-hint variable and matched-facility counts;
- validation result;
- request bounds, used bounds, process elapsed time, and peak RSS when available.

## Decision Boundary

This slice identifies the next measured cliff only. It does not select a reduction or change model
semantics. Once the first cliff is reported, the next decomposition experiment is chosen with the
user before implementation.
