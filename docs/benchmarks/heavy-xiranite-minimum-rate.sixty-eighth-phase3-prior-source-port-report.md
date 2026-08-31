# Phase 3 Prior-Source Port Report

## Result

Fixing the older `item-xiranite-powder` facility source port reduces search activity but does not
separate the Phase 3 five-second cliff.

The target-completion stage again leaves eight unknown leaves. Each is expanded by all five values
of the unique facility-backed source terminal on the same selected logical lanes:

```text
wiring-edge:dc9bd4e56e19e49e58f0bf9175825e166e5c716072c08880f68c098129bc19a6:lane:0000:supply
```

All 40 source-port children remain unknown after five seconds. No child proves infeasibility,
produces a validated witness, or produces an invalid witness. The old source output port does not
act as an outcome separator within the five-second budget in any of the 40 children.

## Exact coverage

The hierarchy covers the selected diagnostic state with disjoint regions:

| Region kind | Count | Outcome |
|---|---:|---|
| Closed demand-pair parents | 17 | Proven infeasible |
| Closed target-completion leaves | 32 | Proven infeasible |
| Source-port leaves | 40 | Unknown |
| Total exact coverage regions | 89 | 49 closed, 40 unknown |

Only `ProvenInfeasible` parents are closed. The eight unresolved target-completion leaves are each
expanded by the complete exact domain `output-belt-0` through `output-belt-4`. Every parent has five
unique children and every source leaf fixes nine distinct facility terminal IDs.

All children use the same exact preceding reference object and retain
`PriorOverlapPlacements`. No prior route, flow, topology, component, or non-selected port value is
copied from the reference.

## Source geometry

The selected source facility is at `(1,11)` with rotation 90 in the shared preceding reference.
The current exact domain and calculated connection cells are:

| Source port | Connection cell | Outcome count |
|---|---:|---:|
| `output-belt-0` | `(6,11)` | 8 unknown |
| `output-belt-1` | `(6,12)` | 8 unknown |
| `output-belt-2` | `(6,13)` | 8 unknown |
| `output-belt-3` | `(6,14)` | 8 unknown |
| `output-belt-4` | `(6,15)` | 8 unknown |

The reference happened to select `output-belt-4`; it is metadata, not a preferred or fixed value in
the parent portfolio.

## Search effect

The source equality adds no variable and one equality/one incidence to each model:

| Variables | Constraints | Incidences | Placement-routing incidences |
|---:|---:|---:|---:|
| 64,471 | 163,830 | 626,032 | 244,622 |

The target-completion parents and source-fixed children use the same five-second budget.

| Search metric | Eight target-completion parents | Forty source-fixed children |
|---|---:|---:|
| Decisions, average | 45,571 | 31,799 |
| Decisions, range | 41,546--54,804 | 25,104--44,408 |
| Conflicts, average | 3,678 | 2,678 |
| Propagations, average | 4,287,110 | 3,002,684 |
| First incumbent | none | none |

The extra equality cuts average decisions by 30.2%, conflicts by 27.2%, and propagations by 30.0%,
but every child still consumes its full time budget. The equality changes the explored search and
lowers the recorded event counts within the fixed budget; this is not evidence of a runtime
improvement. Timing variation across parallel workers is observational rather than a proof of a
stable speed ratio.

Across source children, construction takes 527--990 ms under contention. Search takes
5,006--5,008 ms. Backtracks range from 2,305 to 3,516 and propagations from 2,769,923 to 3,923,784.

## What is now fixed and what is not

All four facility terminals of the raw `item-xiranite-powder` network are fixed in every source
leaf:

- the introduced facility supply `fd54...:supply`;
- the old facility supply `dc9...:supply`; and
- the two target-facility demands `dc9...:demand` and `fd54...:demand`.

There is no external raw-powder terminal in the cumulative Phase 3 model. The apparent external
`fd54...:supply` inside the preceding Phase 2 reference is only the earlier frontier representation;
Phase 3 replaces it with the introduced facility endpoint.

The complete Phase 3 model still has seven unfixed facility terminals:

| Terminal | Current exact port domain |
|---|---|
| `220417...:supply` | `output-pipe-2`, `output-pipe-3` |
| `5ec109...:demand` | `input-pipe-2`, `input-pipe-3` |
| `73f36e...:demand` | `input-pipe-5` |
| `898441...:demand` | `input-belt-0` through `input-belt-4` |
| `9109f3...:demand` | `input-belt-0` through `input-belt-4` |
| `9cc78f...:supply` | `output-pipe-2`, `output-pipe-3` |
| `f959dd...:demand` | `input-pipe-2`, `input-pipe-3` |

The two five-value belt demands belong to the same old powder-producing facility whose output was
just fixed. They affect its other material routes and shared belt-layer congestion. Therefore the
whole-model residual is not yet a routing-only cliff even though the raw-powder network's facility
endpoint geometry is complete.

## Next exact diagnostic

Use one explicitly labelled representative source leaf and partition each of the old facility's two
five-value belt-demand terminals independently. This is two overlapping five-case controls, not ten
disjoint regions and not a proof over all 40 residual source leaves. It asks whether either input
endpoint alone is another strong gate while keeping the other input and all routing decisions free.
The representative leaf and its nine inherited fixations must be selected in the experiment
contract before results are observed.

If neither single-terminal control separates the representative cliff, execute the complete 5 by 5
Cartesian pair for that representative leaf. Only after that pair remains unresolved should the
next slice prioritize root-domain census, per-variable-family decision tracing, and per-network
route/flow propagator counters over another unmeasured propagator.

No external raw-powder boundary-side split is applicable.

## Timing

| Stage | Wall time |
|---|---:|
| Pair preparation and prefix | 31,319 ms |
| 25 pair cases | 12,786 ms |
| 40 target-completion children | 23,944 ms |
| Complete target stage | 68,052 ms |
| Source preparation | <1 ms |
| 40 source-port children | 24,071 ms |
| Complete CLI diagnosis | 92,123 ms |

## Artifact

```text
/tmp/aic-phase3-prior-source-port.9zyJqD
```

The CLI generated `summary.json`, `summary.html`, `stdout.json`, and 40 standalone source-leaf HTML
files.

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
  --output-dir /tmp/aic-phase3-prior-source-port.9zyJqD
```
