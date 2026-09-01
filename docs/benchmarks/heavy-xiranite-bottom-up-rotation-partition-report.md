# Heavy Xiranite Phase 30 Exact Rotation Partition

## Result

The Phase 30 first-witness cliff collapses when one newly introduced seed collector's four
directional rotations are split into four exact solver instances. The unpartitioned propagated
facility-port rung needs 21.973 seconds under High priority and 24.650 seconds under Medium
priority. The four-way portfolio finds a validated witness in a median 644.5 milliseconds under
High and 637.0 milliseconds under Medium.

This is a 34.1x High-priority and 38.7x Medium-priority four-worker wall-clock improvement to the
first witness relative to one prior unpartitioned long run at each priority. It is not a
same-resource latency distribution or CPU-work reduction claim. The portfolio preserves the
entire solution set: the four children fix rotation to 0, 90, 180, or 270 degrees, and their
pairwise-disjoint union is the original Phase 30 problem.

## Exact Contract

Each child adds one `research-fixation` equality for the selected seed collector's directional
rotation. It does not fix or restrict:

- any facility coordinate;
- any other facility rotation;
- any port choice;
- any local connection key or outside connection cell; or
- any point-versus-rectangle clearance choice.

The diagnostic rejects unknown, illegal, duplicated, or non-introduced partition facilities.
The report certifies the enumerated case count, complete Cartesian coverage, and pairwise
disjointness. A feasible child proves parent feasibility. All children would need to prove
infeasible before the parent could be reported infeasible.

The diagnostic deliberately waits for every child so it can preserve all evidence. It separately
records wall time to the first feasible child; an operational first-witness portfolio may cancel
remaining children after that point without changing the witness proof.

## Why Phase 30 Was Selected

Phase 29 is feasible in approximately 0.23 seconds under High priority. Phase 30 adds three 5x5
square facilities: one seed collector and two planters. Their square footprints add no new shape
choice, but their directional rotations change port geometry.

| Metric | Phase 29 | Phase 30 | Delta |
|---|---:|---:|---:|
| Facilities | 30 | 33 | +3 |
| Facility terminals | 95 | 105 | +10 |
| Variables | 4,302 | 4,982 | +680 |
| Constraints | 8,834 | 10,396 | +1,562 |
| Clearance relations | 2,755 | 3,360 | +605 |
| Semantic assignment upper bound | 566.60 bits | 628.96 bits | +62.36 bits |
| Port-choice contribution | 172.98 bits | 196.20 bits | +23.22 bits |

The seed collector owns four of the ten newly introduced endpoints. Partitioning it therefore cuts
one directional choice that fans out into multiple endpoint support and clearance relations while
leaving every downstream physical decision exact.

## Repeated Four-Way Portfolios

Each row summarizes four fresh release-profile portfolios. Every portfolio runs four independent
Pumpkin instances concurrently with a five-second budget per child, diagnostic counters enabled,
the false-event filter disabled, and the same caller-supplied 50x50 ceiling.

| Priority | First-feasible wall range | Median first-feasible wall | Feasible rotations | Full evidence wall |
|---|---:|---:|---|---:|
| High | 634-702 ms | 644.5 ms | 90, 180 | 5.05-5.07 s |
| Medium | 630-735 ms | 637.0 ms | 270 | 5.05 s |

The full evidence wall remains approximately five seconds because some sibling cases time out and
the diagnostic retains them. That does not delay proof of the first feasible child.

## Per-Rotation Search Behavior

| Priority | Rotation | Outcome in all repeats | Search range | Decisions | Conflicts | Solver propagations |
|---|---:|---|---:|---:|---:|---:|
| High | 0 | unknown | 5,000 ms | 225,139-236,184 | 9,968-10,440 | 40.19M-42.37M |
| High | 90 | feasible | 892-1,059 ms | 53,610 | 1,199 | 7,799,263 |
| High | 180 | feasible | 588-661 ms | 33,283 | 676 | 4,908,106 |
| High | 270 | unknown | 5,000 ms | 246,588-256,069 | 8,699-9,213 | 42.42M-44.45M |
| Medium | 0 | unknown | 5,000 ms | 199,376-206,384 | 10,280-10,621 | 33.97M-35.16M |
| Medium | 90 | unknown | 5,000 ms | 226,747-237,183 | 9,486-10,027 | 34.99M-36.83M |
| Medium | 180 | unknown | 5,000 ms | 229,481-241,268 | 8,555-8,990 | 33.54M-35.33M |
| Medium | 270 | feasible | 585-692 ms | 33,814 | 749 | 4,663,370 |

The deterministic search counts of feasible children are much smaller than the unpartitioned
tree. This is not merely four unpartitioned restarts in parallel: each child solves one disjoint
part of the directional-rotation domain. The current evidence does not separate root propagation,
branch ordering, conflict learning, fresh solver instances, and four-core parallelism as causes of
the wall-clock reduction.

## Interpretation

The result implicates a small exact disjunction at the head of this Phase 30 chain:

```text
directional rotation
  -> compatible port/local-connection rows
  -> outside connection coordinates
  -> point-versus-facility clearance
```

The experiment does not prove that rotation partitioning is a permanent solver architecture. It
proves that externally splitting this directional-rotation disjunction is a sufficient exact cut
for Phase 30 first-witness search. It does not yet show whether the monolithic model lacks a useful
propagation rule or merely branches and learns poorly across the disjunction. The 16- and 64-way
planter extensions were intentionally not run because the first 4-way partition already crossed
the diagnostic gate and found sub-second witnesses.

The next diagnostic must distinguish those explanations before adding another propagator. It will
compare root domains in the unpartitioned parent and fixed-rotation children, then census whether
direct local-key clearance can prove any unsupported `(local_key, target_orientation)` pair that
the current model retains. If that opportunity exists, a local-key-aware clearance pilot can
reason about `owner_x + dx`, `owner_y + dy` versus the target rectangle and channel exact pruning
through the existing `(rotation, port, local_key)` support relation. The rotation portfolio remains
the exact baseline and operational fallback for that experiment.

## Verification

- Every feasible child passes the unchanged facility-port witness validator.
- Machine checks confirm each feasible witness uses the rotation fixed by its child.
- All eight reports certify four complete, pairwise-disjoint cases.
- Each child has exactly one `research-fixation` constraint and otherwise matches the propagated
  Rung 1B contract.
- Eight JSON and eight self-contained HTML reports were generated automatically by the release
  CLI.
- A release-profile counter-disabled probe reported
  `endpoint_clearance_counters_enabled=false` for all four children.
- The wrong-rung CLI test confirms partition validation returns its dedicated diagnostic before
  attempting to load input files.
- `cargo fmt --all -- --check` passed.
- `git diff --check` passed.
- `cargo check --workspace` passed.
- `cargo test --workspace` passed: eight ladder CLI tests, 351 `aic-data` tests, and every other
  workspace test completed without failure.

## Artifacts

- `heavy-xiranite-bottom-up-rotation-partition/high-run1/` through `high-run4/`
- `heavy-xiranite-bottom-up-rotation-partition/medium-run1/` through `medium-run4/`
