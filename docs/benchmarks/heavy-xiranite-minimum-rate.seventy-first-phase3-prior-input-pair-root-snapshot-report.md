# Heavy Xiranite Phase 3 Prior-Input Pair Root Snapshot

## Question

After the prior-input pair portfolio left twelve of sixteen exact pair cases unknown at five
seconds, which solver domains are still broad before the first branch in the lowest-index completed
unknown case?

This is a read-only observation. It does not change the exact `16 x 16` pair case, add a hint,
choose a branch, or claim that a broad root domain is the runtime bottleneck.

## Why This Diagnostic Uses 16 x 16

`16 x 16` is not an optimum, default, or game limit. The preceding exact three-facility witness had
used dimensions `7 x 16`. The Phase 3 residual experiments preserve that witness's facility
coordinates, so height 16 is required by the fixed predecessor placement. A square `16 x 16` case
was retained as one controlled diagnostic canvas while the newly introduced facility and its
residual endpoint state were decomposed.

The current fixed facilities occupy coordinates that physically fit from width 13 upward, but a
`13 x 16` case changes the fixed-dimension partition and external-boundary contract. Comparing
`13 x 16` through `16 x 16` is therefore a later exact sensitivity experiment, not an identical
problem A/B.

## Reproduction Contract

- cumulative Heavy Xiranite Phase 3;
- exact used dimensions `16 x 16`;
- all four facility placements fixed;
- eleven facility terminals fixed by the preceding exact portfolios;
- selected residual pair index derived as the lowest completed `Unknown` case;
- every remaining facility port, external terminal, belt/pipe cell, arc, item, flow, topology, and
  logistics-component decision left to Pumpkin;
- feasibility-only search with a five-second observed-case budget;
- the unchanged dynamic/default brancher wrapped by a one-shot read-only root observer.

The derived case is pair index 1. Its uninstrumented baseline and observed solve both return
`Unknown`; no invalid witness or reproduction failure blocks interpretation.

## Domain Coverage

| Metric | Count |
|---|---:|
| Pumpkin domains | 148,063 |
| Registered semantic/model domains | 64,471 |
| Unregistered internal/helper domains | 83,592 |
| Registered coverage | 43.54% |

The family table below is exact for registered variables but is not a complete census of every
Pumpkin domain. In particular, the first delegated decision is the unregistered Boolean predicate
`[x117486 >= 1]`. Pumpkin's default brancher initially falls back to random selection over all
solver domains before conflict activity is available. This observation is useful provenance, but
does not by itself justify a branch-order change.

## Registered Root Domains

| Family | Total | Fixed | Unresolved |
|---|---:|---:|---:|
| arm-item | 18,120 | 8,768 | 9,352 |
| objective | 7,449 | 3,586 | 3,863 |
| branch-component | 4,096 | 1,648 | 2,448 |
| route-arm | 4,096 | 1,844 | 2,252 |
| bridge-rotation | 2,048 | 976 | 1,072 |
| flow | 1,920 | 904 | 1,016 |
| route-arc | 1,920 | 904 | 1,016 |
| boundary-terminal | 5,450 | 4,900 | 550 |
| route-cell | 512 | 212 | 300 |
| transport-occupancy | 512 | 212 | 300 |
| bridge | 512 | 244 | 268 |
| terminal-presence | 4,096 | 3,876 | 220 |
| endpoint-geometry | 10,136 | 10,124 | 12 |
| endpoint | 16 | 12 | 4 |
| physical-occupancy | 1,280 | 1,280 | 0 |
| placement | 2,308 | 2,308 | 0 |

These counts identify candidates only. They must not be ranked as the complete search state because
56.46% of Pumpkin domains are internal or otherwise unregistered.

## Facility And Port State

All four placement-choice domains are singleton, and the reconstructed `x`, `y`, and rotation are
singleton. All eleven explicitly fixed facility terminals satisfy their singleton port and
geometry assertions.

Exactly four facility terminals remain non-singleton. Each is a two-way pipe-port choice on the
liquid-processing facility:

- two inputs choose `input-pipe-2` or `input-pipe-3`;
- two outputs choose `output-pipe-2` or `output-pipe-3`.

Each terminal has exactly two live routing options. The complete remaining facility-port state is
therefore covered by the exact Cartesian product `2^4 = 16`. Some complete tuples may subsequently
be proven infeasible by cross-terminal constraints.

## External-Terminal Channel

There are ten external terminals. For every terminal:

| Metric | Count |
|---|---:|
| Raw parent-key cardinality | 534 |
| Routing-option literals fixed false | 490 |
| Routing-option literals still live | 54 |
| Live unique boundary cells | 50 |
| Live sides | north, east, south, west |

The 534 parent values are not 534 legal semantic choices. The parent starts as the full packed-key
interval of 1,024 values. The unary positive table declares 544 boundary keys, and exact
fixed-dimension and occupancy constraints make 490 of their option literals false. Those 490
exclusions do reach the parent: `1,024 - 490 = 534`. The remaining domain is 54 live boundary keys
plus 480 values that never had a row in the unary table. Pumpkin's positive-table encoding does not
remove those relation-absent values at root. This is direct evidence for an exact unary support
channel or equivalent sparse-domain reformulation, not missing false-option back-propagation.

An independent review found and blocked an earlier reporting error that decoded the raw parent key
as 256 possible boundary cells. The corrected report keeps the raw 534-value domain as propagation
evidence and derives the actual 50 cells only from option literals that can still be true.

## Shared Grid State

| Layer | Route cells T/F/U | Boundary T/F/U | Interior T/F/U | Arcs T/F/U | Flow positive/zero/U |
|---|---:|---:|---:|---:|---:|
| Belt | 9 / 100 / 147 | 2 / 10 / 48 | 7 / 90 / 99 | 0 / 452 / 508 | 0 / 452 / 508 |
| Pipe | 3 / 100 / 153 | 0 / 10 / 50 | 3 / 90 / 103 | 0 / 452 / 508 | 0 / 452 / 508 |

The four 5-by-5 facilities explain the 100 route cells fixed false on each layer. Nearly every
remaining physical cell is unresolved apart from required terminal cells. Every one of the eight
material networks retains 508 material-capable possible directed arcs, and every possible demand
option is reachable from at least one possible supply in this over-approximate root graph.

This confirms that the `16 x 16` empty space is materially represented in the residual route state.
It does not prove that canvas size alone causes the five-second cliff.

## Interpretation

The snapshot rules out placement uncertainty in this leaf and verifies the inherited fixed state.
It leaves three distinct exact candidates:

1. four binary facility-port decisions;
2. 480 relation-absent values retained by each external unary boundary-key table;
3. broad shared route, item, topology, and flow state after endpoints are known.

The smallest complete next decomposition is the 16-case facility-port portfolio. It partitions the
same `16 x 16` feasible set and removes the entire remaining facility-port decision class. If no
assignment yields a witness and every assignment not proven infeasible remains `Unknown`, complete
facility-port fixation still does not cross the cliff; the lowest-index completed unknown child
becomes the control for an exact boundary-key support-channel A/B. If all sixteen assignments are
proven infeasible, the parent pair is proven infeasible instead. A dimension sensitivity sweep
remains useful afterward, but changes the physical and boundary problem and therefore cannot
isolate this channel first.

## Independent Review

Three independent reviews examined observer soundness, arithmetic and artifact interpretation, and
the next exact experiment.

- Accepted: separate raw parent-key state from live external routing options and identify the 480
  relation-absent values rather than blaming already-propagated false options.
- Accepted: record total Pumpkin-domain coverage and state that registered family rankings are
  partial.
- Accepted: use the complete `2^4` facility-port portfolio as the next smallest exact partition.
- Rejected as unsupported: treating 256 raw decoded cells as legal boundary choices.
- Deferred: changing branch order based only on one unregistered first decision.

## Artifact

- `/tmp/aic-phase3-root-snapshot-final.KMqEJy/summary.json`
- `/tmp/aic-phase3-root-snapshot-final.KMqEJy/stdout.json`
- `/tmp/aic-phase3-root-snapshot-final.KMqEJy/summary.html`
- `/tmp/aic-phase3-root-snapshot-final.KMqEJy/observed-layout.html`

The two JSON files are semantically identical, and the HTML files are emitted automatically even
though the observed solve returns `Unknown`.

## Reproduction

```text
target/release/aic-prior-terminal-pair \
  --workload data/benchmarks/heavy-xiranite-minimum-rate.workload.json \
  --workspace-root . \
  --placement-request data/benchmarks/requests/placement.16x16.request.json \
  --target-phase 3 \
  --used-width 16 \
  --used-height 16 \
  --facility-x 8 \
  --facility-y 5 \
  --port-assignment-index 5 \
  --facility-rotation 0 \
  --prior-facility-bit 2 \
  --terminal-pair 2,3 \
  --worker-count 12 \
  --prefix-case-time-limit-ms 10000 \
  --pair-case-time-limit-ms 5000 \
  --complete-target-ports \
  --child-case-time-limit-ms 5000 \
  --split-prior-source-port \
  --source-case-time-limit-ms 5000 \
  --control-prior-input-ports \
  --representative-source-leaf-index 0 \
  --input-control-case-time-limit-ms 5000 \
  --pair-prior-input-ports \
  --input-pair-case-time-limit-ms 5000 \
  --root-domain-snapshot \
  --root-snapshot-case-time-limit-ms 5000 \
  --output-dir /tmp/aic-phase3-root-snapshot-final.KMqEJy
```
