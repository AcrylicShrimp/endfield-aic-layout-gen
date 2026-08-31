# Heavy Xiranite Pair Feasibility Versus Optimization Report

## Outcome

Disabling objective optimization does not restore a first solution for the smallest failing
two-network model. Both the faithful lexicographic optimizer and a research-only first-solution
search returned `unknown` with zero incumbents after 5,000 ms.

The models are structurally identical: 10,970 variables, 33,256 constraints, 109,133 terms, and the
same complete family and coupling metrics. Placement, rotation, port, boundary terminal, routing,
flow, item, topology, capacity, collision, used geometry, and objective-definition state are all
unchanged.

The immediate five-second cliff therefore occurs while trying to construct the first complete
hard-constraint witness. It is not caused by comparing feasible layouts for better area, transport
tile count, turns, or later tie-breakers.

## Controlled Difference

Both cases rebuild Heavy Xiranite phase-0 `pair-0-1`, containing the enriched Xiranite powder belt
network and the Xiranite powder belt network, under the same 12 by 12 diagnostic ceiling.

Only the Pumpkin call differs:

- `optimize` invokes the current lexicographic optimizer;
- `feasibility-only` invokes satisfaction search and stops at the first complete assignment.

Every objective variable and definition constraint remains present in both models. Feasibility-only
changes no legal solution. It is diagnostic-only because it does not demand layout quality.

No placement, rotation, port, boundary point, route, dimension, or objective value was fixed. Both
use Pumpkin's default brancher and the same conflict resolver.

## Result

| Metric | Optimize | Feasibility only |
| --- | ---: | ---: |
| Variables | 10,970 | 10,970 |
| Boolean / integer | 9,402 / 1,568 | 9,402 / 1,568 |
| Log2 domain volume | 12,796.29 | 12,796.29 |
| Constraints | 33,256 | 33,256 |
| Terms | 109,133 | 109,133 |
| Placement-routing constraints | 5,872 | 5,872 |
| Placement-routing incidences | 23,350 | 23,350 |
| Build | 80 ms | 67 ms |
| Search | 5,000 ms | 5,001 ms |
| First incumbent | none | none |
| Objective stages reached | 0 | 0 |
| Termination | unknown | unknown |
| Peak RSS | 86.88 MiB | 85.14 MiB |
| Instructions retired | 87.77 billion | 87.65 billion |
| CPU cycles | 19.13 billion | 19.17 billion |

The complete serialized `model_complexity` reports compare byte-for-byte after canonical JSON key
sorting. Small build-time and RSS differences are ordinary isolated-process observations, not model
changes.

The almost identical instruction and cycle counts show that both modes spend the budget in the same
pre-incumbent region. The optimizer records no objective stage because it never receives an initial
solution whose area could be improved.

## Interpretation

The previous diagnosis established that every single network is tractable and every network pair
crosses the five-second first-incumbent cliff. This experiment further rules out the explanation
that Pumpkin is wasting those five seconds proving or improving compactness.

The remaining blocker is on the shared path used by both search modes:

```text
facility placement and port choices
  <-> external boundary terminals and used dimensions
  <-> multi-item route arms and integer flow
  <-> topology, capacity, bridge, and collision constraints
  -> first complete satisfying witness
```

This result does not identify which hard relation is dominant. It narrows the target from
"optimization versus feasibility" to propagation and branching inside the hard multi-network
model.

## What This Rules Out

- later lexicographic tie-breakers: no objective stage was reached;
- optimization of area after a feasible layout: no feasible layout was found;
- different model size or semantics between modes: all structural metrics are identical;
- memory exhaustion: both processes stay below 87 MiB peak RSS;
- production fallback or heuristic behavior: neither exists in either case.

It does not prove the pair infeasible. Both results remain resource-limited `unknown`.

## One Next Discriminating Experiment

Partition the exact `pair-0-1` problem by `(used_width, used_height)` and solve every legal dimension
case as an independent feasibility problem. The union of all cases must cover the complete original
dimension domain, so this is an exact decomposition rather than a preferred-size heuristic.

This test removes the need for one solver run to choose dimensions while simultaneously placing the
facility and attaching external terminals to the selected used boundary.

- If at least one fixed-dimension case finds a witness quickly, the used-dimension and boundary
  coupling is the immediate search cliff.
- If all exact dimension partitions remain stuck, the target moves inside port, multi-item routing,
  flow, topology, and capacity state.

Do not implement this decomposition until the user reviews the present result and its expected
case count and budget.

## Verification

- `cargo fmt --all`
- `cargo check --workspace`
- `cargo test --workspace`: 179 tests passed
- `cargo build --release --workspace`
- two isolated optimized release processes with equal 5,000 ms budgets
- identical complete `model_complexity` reports
- JSON and self-contained HTML for both failures
- process-isolated `/usr/bin/time -l` RSS, instruction, and cycle measurements
- no production search change, fixed solver decision, heuristic, or fallback

## Artifacts

- Experiment contract: `docs/designs/pair-feasibility-versus-optimization-diagnosis.md`
- Machine-readable comparison:
  `docs/benchmarks/heavy-xiranite-pair-search-mode/comparison.json`
- Raw optimized JSON and HTML:
  `docs/benchmarks/heavy-xiranite-pair-search-mode/optimize.json` and
  `docs/benchmarks/heavy-xiranite-pair-search-mode/optimize.html`
- Raw feasibility-only JSON and HTML:
  `docs/benchmarks/heavy-xiranite-pair-search-mode/feasibility-only.json` and
  `docs/benchmarks/heavy-xiranite-pair-search-mode/feasibility-only.html`

## Decision Boundary

The feasibility-versus-optimization boundary is resolved for the five-second cliff. Production
continues to use lexicographic optimization. Pause here before implementing exact dimension
partitioning or any hard-model reformulation.
