# Heavy Xiranite Phase 30 Rotation Root-Domain Comparison

## Result

Fixing the newly introduced seed collector's directional rotation propagates immediately through
the existing exact rotation-port-local support relation, but it does not reduce port choices,
facility coordinates, endpoint connection coordinates, or point-rectangle clearance domains at
the root fixpoint.

The parent and all four fixed-rotation children were observed before any search decision. The
children reduce the selected facility's rotation domain from four values to one and its four
endpoint local-key cardinalities from a combined 72 to 20. The endpoint-support propagator records
52 removed values in every child. Every other measured aggregate remains unchanged.

## Exact Observation Contract

The diagnostic builds the unchanged propagated facility-port rung. The parent adds no fixation;
each child adds one exact directional-rotation equality. The diagnostic calls Pumpkin's public
`propagate_to_fixpoint()` API directly. Pumpkin runs that path with its `NoDecisionBrancher`, so it
can propagate but cannot post a branch decision. The diagnostic then records every surviving
integer value for:

- facility `x`, `y`, and directional rotation;
- facility geometry-orientation literals;
- endpoint port choice and local-connection key;
- endpoint connection `x` and `y`; and
- the current endpoint-support and clearance propagation counters.

The local-key clearance opportunity census is observational. For every supported local key and
possible target orientation, it checks the Cartesian product of current root coordinate domains.
It reports a pair only when even that superset has no coordinate tuple placing the connection point
outside the target rectangle. The census posts no constraint and removes no value.

## Root-Domain Breakdown

The following domain cardinalities are identical under High and Medium clearance scheduling
priorities.

| Metric | Parent | Every fixed-rotation child | Delta |
|---|---:|---:|---:|
| Selected facility rotation cardinality | 4 | 1 | -3 |
| Selected facility endpoint count | 4 | 4 | 0 |
| Selected endpoint port cardinality sum | 20 | 20 | 0 |
| Selected endpoint local-key cardinality sum | 72 | 20 | -52 |
| Selected endpoint connection-x cardinality sum | 200 | 200 | 0 |
| Selected endpoint connection-y cardinality sum | 200 | 200 | 0 |
| All facility rotation cardinality sum | 132 | 129 | -3 |
| All facility x cardinality sum | 1,578 | 1,578 | 0 |
| All facility y cardinality sum | 1,578 | 1,578 | 0 |
| All endpoint port cardinality sum | 416 | 416 | 0 |
| All endpoint local-key cardinality sum | 1,484 | 1,432 | -52 |
| All endpoint connection-x cardinality sum | 5,250 | 5,250 | 0 |
| All endpoint connection-y cardinality sum | 5,250 | 5,250 | 0 |
| Endpoint-support removed values | 0 | 52 | +52 |
| Clearance rejected orientations | 0 | 0 | 0 |
| Clearance coordinate bound updates | 0 | 0 | 0 |
| Conservative root clearance opportunities | 0 | 0 | 0 |

High-priority root propagation took 0.65-0.76 milliseconds per model; Medium took 0.76-1.07
milliseconds. Model construction took 17.4-19.8 milliseconds. These timings are descriptive, not
a performance benchmark: the parent is always built first and allocator/cache state is not
controlled.

## Interpretation

The existing channel is not missing the direct implication `fixed rotation -> supported local
keys`. It performs that reduction completely at root: all 52 local-key values incompatible with
the fixed rotation disappear immediately.

The large first-witness improvement from the four-way portfolio therefore cannot be explained by
an absent rotation-to-local-key propagation rule. Root fixing does not narrow port choices or any
coordinate domain, and the current clearance propagator has no root-level deduction to make while
all facility coordinates remain wide.

The conservative local-key-aware clearance pilot proposed in the previous report has no observed
root opportunity on this workload. This rules out only that coarse root-level local-key rejection
rule; it does not rule out coordinate-value, bound, or correlation-aware clearance propagation
during search. Implementing a pruning rule now would be speculation. The next exact diagnostic
should observe domain transitions and the same opportunity predicate during search, after
coordinate domains have narrowed. If dynamic missed opportunities appear frequently before the
cliff, a redundant semantic propagator has evidence. If they remain absent, the next target is the
exact disjunctive partition/orchestration behavior rather than this clearance rule.

A hand-selected branch order for this particular facility is intentionally not implemented. It is
a search heuristic under the repository policy and requires explicit user approval even though it
would preserve feasibility.

## Reproducibility

- High and Medium release-profile reports both certify four complete, pairwise-disjoint children.
- Both reports record their clearance priority, counter setting, and false-event-filter setting as
  machine-readable provenance.
- After removing timings and propagation counters, their serialized root domains are byte-for-byte
  identical.
- The fixed-rotation unit fixture observes a singleton root rotation domain.
- An asymmetric `2x4` fixture under a `3x4` ceiling confirms that the exact partition includes the
  fitting `0` rotation and excludes the non-parent `90` rotation.
- A direct oracle test covers both a point forced inside a target rectangle and a control domain
  with an outside placement.
- JSON and self-contained HTML reports are emitted automatically by the CLI.

## Independent Review Resolution

The first independent soundness review blocked the slice because the initial observer stopped the
normal solve loop only after Pumpkin had already made one decision. The observer was replaced with
the direct root-fixpoint API and all evidence was regenerated. The same review found that allowed
but non-fitting rotations could create children absent from the parent model; the shared exact
partition helper now filters to the actual fitting parent rotation domain, with the asymmetric
regression above. Reviewers also requested explicit machine-readable search-profile provenance,
which is now present in both reports.

## Verification

The final worktree passed:

```text
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
```

The workspace test result includes 353 `aic-data` tests, 9 bottom-up ladder CLI tests, 35 main CLI
tests, and 6 prior-terminal-pair CLI tests, all passing. Both release-profile artifacts were then
regenerated with:

```bash
partition_instance=$(jq -r '.partitioned_rotation_domains | keys[0]' \
  docs/benchmarks/heavy-xiranite-bottom-up-rotation-root/summary.json)

target/release/aic-bottom-up-ladder \
  --rung facility-ports-propagated \
  --partition-facility "$partition_instance" \
  --partition-root-snapshot \
  --workload data/benchmarks/heavy-xiranite-minimum-rate.workload.json \
  --placement-request data/benchmarks/requests/placement.50x50.request.json \
  --target-phase 30 \
  --time-limit-ms 5000 \
  --output-dir docs/benchmarks/heavy-xiranite-bottom-up-rotation-root

target/release/aic-bottom-up-ladder \
  --rung facility-ports-propagated \
  --endpoint-clearance-priority medium \
  --partition-facility "$partition_instance" \
  --partition-root-snapshot \
  --workload data/benchmarks/heavy-xiranite-minimum-rate.workload.json \
  --placement-request data/benchmarks/requests/placement.50x50.request.json \
  --target-phase 30 \
  --time-limit-ms 5000 \
  --output-dir docs/benchmarks/heavy-xiranite-bottom-up-rotation-root-medium
```

Both commands completed successfully. A normalized comparison that removes construction/root
timings and propagator counters reports identical parent and child domain snapshots across High
and Medium scheduling.

## Artifacts

- `heavy-xiranite-bottom-up-rotation-root/summary.json`
- `heavy-xiranite-bottom-up-rotation-root/summary.html`
- `heavy-xiranite-bottom-up-rotation-root-medium/summary.json`
- `heavy-xiranite-bottom-up-rotation-root-medium/summary.html`
