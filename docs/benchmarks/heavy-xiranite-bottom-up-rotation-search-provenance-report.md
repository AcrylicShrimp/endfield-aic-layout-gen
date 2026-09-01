# Heavy Xiranite Phase 30 Rotation Search Provenance

## Result

The exact four-way rotation partition is a sufficient cut for the Phase 30 first-witness cliff. It
removes both unresolved mixed-case exploration and repeated re-entry into directional-rotation
cases. The current evidence does not isolate either behavior as the sole cause. The improvement is
not a parallel lucky-worker effect and is not explained by a missing root implication from
rotation to endpoint local keys.

The unpartitioned Phase 30 parent found no witness within five seconds. At brancher fixpoints, the
new seed collector's rotation was observed entering a singleton domain 75 times and widening again
75 times. The winning `180`-degree case was first observed only after 9,241 branch decisions and
was observed entering 25 times. In contrast, a fresh exact child with `rotation = 180` found a
witness after 33,283 decisions and 449 milliseconds. The `90`-degree child also found a witness
after 53,610 decisions and 728 milliseconds. The `0`- and `270`-degree children remained unknown at
the same five-second budget.

This is an exact decomposition result. The four children are complete and pairwise disjoint, and
their union is the original parent. No placement, port, coordinate, or endpoint decision is fixed
apart from the one case-defining rotation equality.

## Observation Contract

The search observer wraps Pumpkin's unchanged default brancher. For every decision request it:

- observes the current propagation fixpoint;
- calls the wrapped brancher exactly once;
- returns the same predicate without consuming randomness or adding a constraint;
- records the target rotation domain using exact membership checks;
- classifies the returned decision by the existing variable catalog; and
- forwards the wrapped brancher's subscribed events unchanged.

Full variable-family domain snapshots are limited to the root, powers of two in the decision
count, and the first singleton entry for each rotation. Detailed predicate records are capped at
256. Target rotation transitions are capped at 1,024; this run observed 569 brancher-fixpoint
transitions and dropped none. Transient states that conflict before the next branch request are not
observable. Trace wall time is descriptive because domain observation consumes part of the same
wall-clock budget. Untraced controls remain the performance baseline.

## Parallelism Control

The exact children were run three times sequentially with one worker and three times with four
workers. The two feasible children made exactly the same decisions, conflicts, and propagations in
every run and in both worker configurations. Parallel execution made their wall time longer because
they shared CPU with the two five-second unknown children.

| Mode | Repeat | Rotation | Outcome | Search ms | Decisions | Conflicts | Propagations |
|---|---:|---:|---|---:|---:|---:|---:|
| parent | 1 | free | unknown | 5,000 | 240,705 | 13,790 | 50,744,841 |
| parent | 2 | free | unknown | 5,000 | 214,502 | 12,218 | 44,844,782 |
| parent | 3 | free | unknown | 5,000 | 227,993 | 13,020 | 47,911,854 |
| sequential | 1 | 90 | feasible | 706 | 53,610 | 1,199 | 7,799,263 |
| sequential | 2 | 90 | feasible | 731 | 53,610 | 1,199 | 7,799,263 |
| sequential | 3 | 90 | feasible | 775 | 53,610 | 1,199 | 7,799,263 |
| parallel | 1 | 90 | feasible | 915 | 53,610 | 1,199 | 7,799,263 |
| parallel | 2 | 90 | feasible | 1,034 | 53,610 | 1,199 | 7,799,263 |
| parallel | 3 | 90 | feasible | 1,063 | 53,610 | 1,199 | 7,799,263 |
| sequential | 1 | 180 | feasible | 434 | 33,283 | 676 | 4,908,106 |
| sequential | 2 | 180 | feasible | 461 | 33,283 | 676 | 4,908,106 |
| sequential | 3 | 180 | feasible | 479 | 33,283 | 676 | 4,908,106 |
| parallel | 1 | 180 | feasible | 576 | 33,283 | 676 | 4,908,106 |
| parallel | 2 | 180 | feasible | 651 | 33,283 | 676 | 4,908,106 |
| parallel | 3 | 180 | feasible | 687 | 33,283 | 676 | 4,908,106 |

The sequential four-child portfolio took 11.2-11.4 seconds because it ran both unknown children to
their five-second limits. The parallel portfolio completed in 5.04-5.05 seconds. Parallelism is an
orchestration latency benefit, but it did not cause either feasible witness.

## Search-Time Breakdown

The traced parent made 236,236 decisions, 13,514 conflicts, and 49,767,182 solver propagations. Its
decision-family histogram is complete:

| Variable family | Decisions | Share |
|---|---:|---:|
| endpoint | 725 | 0.3% |
| endpoint geometry | 144,355 | 61.1% |
| placement | 91,156 | 38.6% |

The all-decision counter reports exactly two predicates returned directly on the target rotation
domain, or less than 0.001% of all decisions. The transition origin separately reports whether the
immediately preceding returned predicate targeted rotation; it does not observe direct predicates
that conflict before a later brancher fixpoint.

Only 6,006 parent decisions occurred inside the observed singleton episodes, or 2.5% of the
236,236 total. The remaining 97.5% occurred while the target rotation was unresolved. Re-entry is
therefore one observed symptom of mixed-case search, not a complete accounting of its cost.

| Rotation | First singleton decision | Singleton episodes | Decisions inside episodes | Longest episode | Median episode | Conflicts inside episodes |
|---:|---:|---:|---:|---:|---:|---:|
| 0 | 42 | 5 | 714 | 541 | 61 | 21 |
| 90 | 2,724 | 3 | 70 | 48 | 21 | 5 |
| 180 | 9,241 | 25 | 1,672 | 559 | 30 | 97 |
| 270 | 853 | 42 | 3,550 | 767 | 31 | 157 |

An observed episode begins when a brancher fixpoint first shows the target rotation as a singleton
and ends when a later brancher fixpoint shows it widened. These counts are a lower bound on trail
transitions, not disjoint solver subtrees: transient conflicting states may be invisible, and
learned clauses and other trail state survive ordinary backtracking according to Pumpkin's search
semantics.

## State At First Entry

The first observed entry into each rotation happens with materially different prior commitments.
The two winning rotations, `90` and `180`, are first reached after 2,724 and 9,241 decisions. The
early `0` entry is an important control: not every rotation requires substantial prior work.
Together these states show that the parent is not simply trying four clean rotation children in
sequence.

| Rotation | Decision | Target x values | Target y values | Target port sum | Unresolved endpoint vars | Unresolved endpoint-geometry vars | Unresolved placement vars |
|---:|---:|---:|---:|---:|---:|---:|---:|
| 0 | 42 | 46 | 45 | 16 | 95 | 1,478 | 3,024 |
| 270 | 853 | 2 | 46 | 16 | 90 | 1,178 | 1,996 |
| 90 | 2,724 | 3 | 46 | 16 | 72 | 780 | 1,768 |
| 180 | 9,241 | 3 | 45 | 16 | 65 | 613 | 2,087 |

The prior root-domain experiment already proved that fixing this rotation immediately removes all
52 incompatible target local-key values but does not narrow port choices, facility coordinates,
endpoint coordinates, or clearance state at the root. The search trace adds behavioral evidence:
the free parent spends most decisions with rotation unresolved and repeatedly enters and abandons
exact cases. The winning cases are not reached as clean root subproblems. It does not prove whether
mixed-case branching, fresh trail state, learned-state history, or lower per-node propagation cost
contributes most of the child improvement.

## Interpretation

The next exact improvement target is orchestration over a complete directional-rotation
partition, not another speculative local-key clearance propagator. Rotation partitioning is
semantics-preserving because it enumerates every fitting rotation, but it creates multiple fresh
solver instances and therefore changes search organization and learned-clause sharing. That is an
algorithmic tradeoff to measure while growing the graph, not a proof that every future phase should
always partition every facility.

The fixed children also perform less propagation per decision: approximately 145-147 solver
propagations per decision in the feasible `90` and `180` children, versus approximately 211 in the
traced parent. Root local-key reduction, fresh activity and trail state, branching behavior, and
learning can all contribute. The next growth run uses the partition because it is demonstrably
sufficient, not because this slice has selected one explanation as the exclusive cause.

A rotation-first custom branch order is not inferred from this result. It would be a heuristic
search-policy change under the repository rules and is neither necessary for this exact
decomposition nor approved here.

The next growth experiment retains the same seed collector's complete four-way partition while
increasing the cumulative phase. It records unpartitioned control time, first-feasible and
full-coverage wall time, feasible and unknown child counts, and summed decisions, conflicts, and
propagations. It must not automatically add every new facility rotation to the Cartesian product.
If the same four children stop yielding a witness or total work rises sharply, that first phase is
the next decomposition target.

## Limitations

- The trace observes brancher fixpoints, not the interior of individual propagator calls.
- `backtrack_callbacks` can include result or timeout cleanup and therefore need not equal native
  conflict count exactly.
- The traced parent reports 18 native solver restarts but zero observed brancher restart
  callbacks. Pumpkin 0.5 does not deliver its engine restart path through this callback surface.
  Native search statistics are authoritative; callback observations and restart-aware transition
  claims are not used to derive the result.
- The five-second parent decision count varies with wall-clock throughput. The deterministic child
  decision counts are the stronger parallelism-control evidence.
- A feasible child proves parent feasibility. Unknown sibling cases do not become infeasibility or
  optimality evidence.

## Reproducibility

The release trace was generated with:

```bash
partition_instance=$(jq -r '.partitioned_rotation_domains | keys[0]' \
  docs/benchmarks/heavy-xiranite-bottom-up-rotation-root/summary.json)

target/release/aic-bottom-up-ladder \
  --rung facility-ports-propagated \
  --partition-facility "$partition_instance" \
  --partition-search-provenance \
  --trace-detailed-decisions 256 \
  --workload data/benchmarks/heavy-xiranite-minimum-rate.workload.json \
  --placement-request data/benchmarks/requests/placement.50x50.request.json \
  --target-phase 30 \
  --time-limit-ms 5000 \
  --output-dir docs/benchmarks/heavy-xiranite-bottom-up-rotation-search-provenance
```

The CLI emits both machine-readable JSON and a self-contained HTML table. The trace report
certifies one parent, four complete children, pairwise disjoint fixed-rotation assignments, a
decision-family histogram with zero unrecorded decisions, 569 retained target transitions, and
zero dropped target transitions. The parallelism-control JSON preserves all three parent, three
sequential-portfolio, and three parallel-portfolio trials.

## Independent Review Resolution

Three independent reviews examined observer soundness, implementation and diagnostic coverage,
and optimization interpretation. Their initial reviews blocked the slice on three issues:

- the report attributed the improvement too strongly to repeated case re-entry;
- the bounded decision prefix could not support a claim about all direct rotation decisions; and
- Pumpkin's native restarts were not delivered through the brancher restart callback.

The report now treats exact partitioning only as a sufficient cut and retains the alternative
causes. The observer counts target-rotation predicates across every returned decision and reports
exactly two in the parent. Restart fields are explicitly named callback observations, while native
search statistics are authoritative. The regenerated artifact also certifies zero unrecorded
decisions. All three reviewers returned PASS after inspecting the fixes and regenerated evidence.

## Verification

The final worktree passed:

```text
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
git diff --check
```

The workspace tests include 353 `aic-data` tests, 10 bottom-up ladder CLI tests, 35 main CLI tests,
and 6 prior-terminal-pair CLI tests. Both JSON artifacts parse successfully, and the release CLI
regenerated the self-contained HTML and machine-readable provenance report after the final schema
change.

## Artifacts

- `heavy-xiranite-bottom-up-rotation-search-provenance/summary.json`
- `heavy-xiranite-bottom-up-rotation-search-provenance/summary.html`
- `heavy-xiranite-bottom-up-rotation-search-provenance/parallelism-control.json`
