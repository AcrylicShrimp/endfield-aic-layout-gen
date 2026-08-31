# Integrated Positive-Table Endpoint Report

## Question

Does replacing only the nested facility endpoint `Element` channel with complete positive tables
break the actual Heavy Xiranite Phase 3 first-feasible cliff?

This experiment uses the cumulative four-facility Phase 3 model at an exact `16 x 16` used size.
It is feasibility-only, gives each fresh release process 5,000 ms of search, supplies no hint, and
leaves placement, rotation, port selection, external terminals, routing, flow, occupancy, capacity,
crossing, and topology as solver decisions.

## Result

No. Both encodings timed out in all three runs without an incumbent. The positive table changed the
search tree and strengthened the isolated endpoint relation, but the integrated model consumed more
CPU instructions and memory while preserving the same first-feasible cliff.

| Median metric | Nested Element | Positive table | Change |
|---|---:|---:|---:|
| Outcome | unknown | unknown | no incumbent |
| Construction | 466 ms | 514 ms | +10.3% |
| Search | 5,011 ms | 5,011 ms | unchanged |
| Decisions | 3,067 | 11,113 | +262.3% |
| Backtracks | 234 | 122 | -47.9% |
| Conflicts | 233 | 121 | -48.1% |
| Learned clauses | 233 | 121 | -48.1% |
| Registered propagator calls | 3,351,107 | 2,018,299 | -39.8% |
| Maximum RSS | 458,735,616 bytes | 543,326,208 bytes | +18.4% |
| Retired instructions | 97,253,192,783 | 113,326,943,301 | +16.5% |
| CPU cycles | 20,691,292,626 | 21,110,942,193 | +2.0% |

Pumpkin's `propagations` statistic is the number of registered propagator calls. It is not the
number of atomic inferences and excludes the table encoder's clause work from the same accounting
category. It therefore cannot be read as a 39.8% reduction in total propagation work. Retired
instructions show that total CPU work increased.

## Encoding cost

The integrated model contains 16 facility endpoint relations and 29,568 legal
`(placement, port, geometry)` rows. The legal rows were enumerated directly from this Phase 3 input.

| Authored or generated state | Nested Element | Positive table |
|---|---:|---:|
| Recorder-visible variables | 64,527 | 64,471 |
| Recorder-visible constraints | 163,873 | 163,817 |
| Recorder-visible terms | 635,771 | 635,619 |
| Table-specific hidden row literals | 0 | 29,568 |
| Table-specific generated clauses | 0 | 107,608 |
| Minimum effective domains after table expansion | 64,527 | 94,039 |

The generic model recorder observes authored repository variables but not variables created inside
Pumpkin constraints. The positive-table model is therefore not smaller: Pumpkin 0.5 creates exactly
one Boolean row selector per row and clauses linking every selector to its three column values.

## Interpretation

The previous channel-only diagnostic remains valid: the table removes unsupported placement values
that the nested `Element` chain retains. The integrated result shows that the standard table is too
expensive to turn that stronger propagation into a Phase 3 improvement.

The table case makes 3.62 times as many decisions while reaching roughly half as many conflicts and
backtracks. Local Pumpkin source inspection establishes that:

1. every table row creates a Boolean selector;
2. the default brancher is constructed over every solver domain after table posting; and
3. row selectors that appear in conflicts can enter VSIDS selection.

This strongly supports, but does not yet prove, the next blocker hypothesis: the 29,568 hidden row
selectors dilute or redirect generic search. Aggregate statistics cannot distinguish selector
decisions from authored-variable decisions, and a stronger relation can also change the tree for
other reasons.

## Correctness and review

Three independent read-only reviews were requested before finalization.

- The soundness review passed: the table enumerates exactly the same complete endpoint tuples,
  preserves one shared placement authority across all facility terminals, preserves rotated port
  direction and port identity, excludes only already-impossible `-1` geometry, and leaves external
  boundary terminals unchanged.
- The experiment review accepted the six-run comparison but blocked the first report draft. It
  found that table overhead had been attributed to both encodings and that registered propagator
  calls had been described as total propagation work. Both defects were corrected and the matrix
  was rerun.
- The strategy review confirmed the hidden-selector mechanism in Pumpkin source and separated it
  from the still-unproven claim that selector decisions dominate the observed search.

A small known-feasible Phase 0 `7 x 7` regression was also solved by both encodings. Nested Element
found and independently validated a witness in 303 ms; positive table did so in 87 ms. This verifies
that the integrated table path can produce a valid complete witness, not only timeout reports.

## Decision

Do not cut over the authoritative endpoint encoding to Pumpkin's standard positive table. Keep the
research-only comparison path because it is a useful propagation oracle, but stop micro-optimizing
it. The measured integrated effect is not an improvement.

The next cliff diagnostic should track the provenance of decisions without changing search policy:

- post a research-only table-equivalent encoding while retaining the created row-selector IDs;
- count selectors fixed at root and selectors still available before search;
- classify decisions as row-selector, authored, or other hidden state;
- record selector polarity and maximum consecutive selector-decision run; and
- preserve the same Phase 3 `16 x 16`, five-second, feasibility-only model.

If selector decisions dominate, the next exact formulation target is a sparse semantic endpoint
support propagator without one branchable literal per legal tuple. If they do not, the hypothesis is
rejected and the next target is the table clause/support-processing cost or a different residual
routing/flow cliff. No branch-order policy should be changed in this diagnostic.

## Artifacts

- `/tmp/aic-integrated-endpoint-final.9SpBTa/nested-element/run-{1,2,3}`
- `/tmp/aic-integrated-endpoint-final.9SpBTa/positive-table/run-{1,2,3}`
- `/tmp/aic-integrated-endpoint-regression.XwKDqG`
- Release binary SHA-256:
  `64ca951f4361ca0acca141666c685844491d761ee631bbd25c8335d819e7954c`
- Source base before this slice: `051268e592d1c45f3adf947a5d7efe5c7c3a2aa8`

Each matrix run directory contains `summary.json`, `summary.html`, `layout.html`, `stdout.json`,
and macOS `/usr/bin/time -l` output in `time.txt`.

## Reproduction

```text
target/release/aic-cli research compare-integrated-endpoint-channel \
  --workload data/benchmarks/heavy-xiranite-minimum-rate.workload.json \
  --workspace-root . \
  --placement-request data/benchmarks/requests/placement.16x16.request.json \
  --target-phase 3 \
  --used-width 16 \
  --used-height 16 \
  --encoding <nested-element|positive-table> \
  --case-time-limit-ms 5000 \
  --output-dir <fresh-directory>
```

Run the command three times per encoding in isolated release processes and wrap each invocation in
`/usr/bin/time -l` for RSS, retired instructions, and cycle measurements.

