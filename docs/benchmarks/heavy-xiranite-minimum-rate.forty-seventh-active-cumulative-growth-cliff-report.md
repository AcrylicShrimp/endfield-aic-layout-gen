# Active Exact Cumulative Growth Cliff

## Scope

This slice connects the reviewed possible-graph and event-driven local-continuation propagator stack to the exact parallel used-dimension portfolio. It then grows the Heavy Xiranite cumulative SCC workload until the next first-feasible cliff appears.

The experiment preserves the joint solver contract:

- facility placement, rotation, port selection, belt/pipe routing, item assignment, topology, capacity, and flow remain solver decisions;
- every used-dimension case is an exact partition of the original bounded problem;
- prior layouts are non-binding hints only;
- no placement, port, corridor, or route heuristic is introduced.

The report schema now records the selected solver stack and cumulative facility-instance IDs so later cliff comparisons can identify the exact logical growth delta.

## Commands

The default growth budget requested by the user is five seconds per exact dimension case:

```text
cargo build --release -p aic-cli
target/release/aic-cli research sweep-cumulative-scc-fixed-dimensions \
  --workload data/benchmarks/heavy-xiranite-minimum-rate.workload.json \
  --workspace-root . \
  --placement-request data/benchmarks/requests/placement.16x16.request.json \
  --target-phase 3 \
  --worker-count 12 \
  --case-time-limit-ms 5000 \
  --active-local-continuation \
  --output-dir docs/benchmarks/heavy-xiranite-active-growth-cliff-16x16-5s
```

A separate ten-second run was used only to distinguish a budget-limited Phase 2 result from the next structural cliff. It is not a new default.

## Five-second growth result

| Phase | Facilities | Networks | Executed cases | Feasible | Proven infeasible | Unknown | Outer wall | Best area | Area proof |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| 0 | 1 | 3 | 21 | 8 | 13 | 0 | 2,560 ms | 42 | proven |
| 1 | 2 | 6 | 37 | 4 | 32 | 1 | 7,603 ms | 77 | proven |
| 2 | 3 | 8 | 100 | 0 | 36 | 64 | 38,154 ms | none | unresolved |

The five-second default therefore exposes a budget cliff at Phase 2. Its aggregate Phase 2 search performed 341,817 branch decisions, 23,785 backtracks, 23,759 conflicts, 23,759 learned clauses, and 207,186,276 solver propagations without a witness.

## Ten-second disambiguation result

| Phase | Facilities | Networks | Executed cases | Feasible | Proven infeasible | Unknown | Outer wall | Best area | Area proof |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| 0 | 1 | 3 | 21 | 8 | 13 | 0 | 2,693 ms | 42 | proven |
| 1 | 2 | 6 | 37 | 5 | 32 | 0 | 8,809 ms | 77 | proven |
| 2 | 3 | 8 | 52 | 1 | 42 | 9 | 23,521 ms | 112 | unresolved |
| 3 | 4 | 8 | 70 | 0 | 17 | 53 | 59,662 ms | none | unresolved |

Phase 2 is not a semantic infeasibility cliff: the larger diagnostic budget finds a validated 7x16 witness with objective `(area=112, transport tiles=39, turns=14, maximum side=16, components=0)`.

Phase 3 is the next structural first-feasible cliff observed by this run. It performs 485,435 branch decisions, 23,238 backtracks, and 287,344,667 solver propagations without a witness.

## Growth delta at the new cliff

The Phase 2 to Phase 3 transition adds:

- one 5x5 facility, increasing required facility area from 75 to 100;
- zero new commodity networks, leaving the network count at eight;
- six new terminals, increasing cumulative terminals from 20 to 26;
- one additional frontier-cut logical edge, increasing that count from four to five.

The 16x16 case itself remains `unknown`, not proven infeasible. Since the four facilities occupy 100 of 256 cells, this result does not support a search-bound shortage diagnosis.

The strongest current hypothesis is therefore not raw facility count alone. The added facility expands placement and compatible endpoint choices inside the same eight shared routing networks. That increases the coupling among placement, port assignment, terminal selection, shared route state, and flow without adding a separable network.

## Conclusion and next experiment

The active exact propagator stack moves the meaningful structural cliff from the controlled three-facility case to the four-facility cumulative model when enough diagnostic time is supplied. Under the standard five-second budget, Phase 2 remains the operational cliff.

The next slice should compare the exact Phase 2 and Phase 3 models and partition only the newly introduced coupling. It should measure, in order:

1. the new facility's placement and compatible port domains;
2. the six added terminal domains and their overlap with existing same-item networks;
3. per-network possible-graph reachability and unresolved local continuation counts;
4. whether the added facility creates interchangeable producer/consumer endpoint assignments or weak source-demand ownership propagation;
5. the smallest exact fixation or redundant valid rule that collapses first-feasible search.

No new propagator should be chosen before this decomposition identifies the failing semantic implication.

## Verification

```text
cargo fmt --all
cargo check --workspace
cargo test --workspace
cargo build --release -p aic-cli
```

All 25 CLI tests and 217 data-library tests passed before the active-stack harness test was added. The final verification reruns the complete workspace suite with that additional controlled active-stack assertion.
