# Heavy Xiranite Minimum-Rate Phase 3 Residual Facility-Port Tuple Report

## Result

The exact Cartesian partition of the four remaining binary facility-port domains reduced the
selected Phase 3 parent from one five-second `Unknown` region to:

- 14 children proven infeasible;
- 2 children still `Unknown` after the standard five-second authoritative budget;
- 0 validated feasible children;
- 0 invalid witnesses;
- 0 fixed-state assertion failures.

This is a meaningful cliff localization, but it is not a feasibility or infeasibility conclusion
for the selected parent. Cases 6 and 12 remain unresolved.

## Contract

The experiment reproduced the accepted fixed `16 x 16` Phase 3 parent, including four fixed
facility placements and eleven previously selected facility terminals. The root observer supplied
the actual two surviving pipe-port IDs for each of four remaining facility terminals. Every one of
the `2^4 = 16` tuples was solved without excluding any tuple in advance.

Each tuple ran two independent copies of the same exact model:

1. an uninstrumented authoritative solve with a 5,000 ms search budget;
2. a separate root-observation solve with a 5,000 ms search budget.

Authoritative timings and counters below come only from the first solve. Logical proof and witness
aggregation preserves sound evidence from either solve. Placement, external terminals, route
geometry, item state, flow, topology, capacity, and collision remained solver decisions. Objective
auxiliary state remained modeled, but these first-witness diagnostic solves used feasibility-only
search and did not optimize that state.

## Aggregate Measurements

| Metric | Value |
| --- | ---: |
| Tuple count | 16 |
| Fixed facility terminals per child | 15 |
| Workers | 12 |
| Authoritative wave wall time | 11,700 ms |
| Observation wave wall time | 11,686 ms |
| End-to-end wall time, including parent reproduction | 138,674 ms |
| Authoritative feasible / infeasible / unknown / invalid | 0 / 14 / 2 / 0 |
| Combined feasible / infeasible / unknown / invalid | 0 / 14 / 2 / 0 |
| Variables per child | 64,471 |
| Constraints per child | 163,836 |
| Incidences per child | 626,038 |
| Placement-routing incidences per child | 244,622 |

The authoritative and observation copies had identical variable and constraint counts for all
sixteen children. Their outcomes also agreed in every case. Relative to the parent, each child had
the same 64,471 variables and exactly four additional equality constraints and incidences, one per
new fixed facility port. Placement-routing incidences remained 244,622.

## Case Breakdown

The four port columns follow the report's stable lexicographic terminal order. `2` and `3` denote
the corresponding input or output pipe-port suffix.

| Case | Ports | Outcome | Search ms | Decisions | Backtracks | Conflicts | Learned | Propagations |
| ---: | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | O2 / I2 / O2 / I2 | Infeasible | 53 | 0 | 0 | 1 | 1 | 168,902 |
| 1 | O3 / I2 / O2 / I2 | Infeasible | 54 | 0 | 0 | 1 | 1 | 168,903 |
| 2 | O2 / I3 / O2 / I2 | Infeasible | 58 | 0 | 0 | 1 | 1 | 175,676 |
| 3 | O3 / I3 / O2 / I2 | Infeasible | 2,508 | 21,779 | 2,035 | 2,036 | 2,036 | 2,249,286 |
| 4 | O2 / I2 / O3 / I2 | Infeasible | 60 | 0 | 0 | 1 | 1 | 168,903 |
| 5 | O3 / I2 / O3 / I2 | Infeasible | 60 | 0 | 0 | 1 | 1 | 168,904 |
| 6 | O2 / I3 / O3 / I2 | Unknown | 5,006 | 47,730 | 4,317 | 4,316 | 4,316 | 4,903,718 |
| 7 | O3 / I3 / O3 / I2 | Infeasible | 64 | 0 | 0 | 1 | 1 | 175,680 |
| 8 | O2 / I2 / O2 / I3 | Infeasible | 62 | 0 | 0 | 1 | 1 | 175,682 |
| 9 | O3 / I2 / O2 / I3 | Infeasible | 710 | 4,911 | 394 | 395 | 395 | 667,594 |
| 10 | O2 / I3 / O2 / I3 | Infeasible | 57 | 0 | 0 | 1 | 1 | 168,906 |
| 11 | O3 / I3 / O2 / I3 | Infeasible | 55 | 0 | 0 | 1 | 1 | 168,907 |
| 12 | O2 / I2 / O3 / I3 | Unknown | 5,008 | 57,990 | 4,476 | 4,475 | 4,475 | 5,425,839 |
| 13 | O3 / I2 / O3 / I3 | Infeasible | 44 | 0 | 0 | 1 | 1 | 175,686 |
| 14 | O2 / I3 / O3 / I3 | Infeasible | 37 | 0 | 0 | 1 | 1 | 168,907 |
| 15 | O3 / I3 / O3 / I3 | Infeasible | 36 | 0 | 0 | 1 | 1 | 168,908 |

Twelve children were rejected during root propagation. Cases 3 and 9 required search before
infeasibility was proven. Cases 6 and 12 remained unresolved and found no incumbent.

The twelve concurrent workers share CPU resources. Search counters therefore characterize this
portfolio run and should not be read as isolated single-worker throughput. This contention does not
weaken a completed infeasibility proof or change the five-second `Unknown` classification.

## Interpretation

The four binary facility-port choices form only sixteen semantic tuples, but leaving them inside
the monolithic search concealed substantial pruning. Once the exact tuple is posted, Pumpkin can
reject most combinations immediately. Therefore the important result is not merely that sixteen
cases are fewer than one broad model. It is that the current formulation does not propagate the
eventual contradiction from fourteen port tuples early enough while those choices remain joined
to routing.

Complete port fixation does not eliminate the Phase 3 cliff. It localizes it to two exact port
tuples. Those children still execute roughly 48--58 thousand decisions and 4.9--5.4 million solver
propagations without producing a first witness or proof in five seconds. The next blocker is inside
the remaining external-terminal and routing state, not in unresolved facility placement or
facility-port assignment.

This experiment does not show that `16 x 16` is an appropriate production size. It is a controlled
continuation of the accepted parent. The eager route model still materializes state for all 256
cells, so a later exact `13 x 16` through `16 x 16` sensitivity experiment remains useful. That is
a separate question from the present port-tuple partition because changing the boundary changes
external-terminal and routing domains.

## Next Exact Target

Use case 6, the lowest-index remaining `Unknown`, as the deterministic diagnostic control. The
next experiment should inspect and then exactly partition or reformulate the broad external
boundary-terminal key domains. The previous root snapshot showed that each external key retained
480 integer values with no legal unary-table support. An exact legal-support representation can
remove those values without excluding a legal solution.

After the boundary-support A/B, rerun growth to find the next cliff. Run the fixed-height width
sensitivity separately after the controlled formulation comparison so that canvas-size effects do
not confound the boundary-key result.

## Independent Review

Independent soundness, measurement, and next-strategy reviews were requested against the source,
design, and release artifact. The measurement review confirmed that authoritative counters are
copied only from the uninstrumented solve, all sixteen model pairs have identical scale, the tuple
enumeration is complete, and the aggregate arithmetic reproduces the raw cases. The strategy
review agreed that the result localizes but does not solve the cliff, and selected the exact
boundary-key legal-support A/B as the next controlled experiment. No accepted review finding
changes solver semantics or requires a heuristic restriction.

## Artifact

The release artifact is `/tmp/aic-phase3-residual-ports.lboaaU`. It contains `summary.json`,
`summary.html`, stdout JSON, and authoritative plus observation HTML for all sixteen cases.

The JSON in `summary.json` and stdout is semantically identical. The standalone files differ only
by the trailing newline written to stdout.
