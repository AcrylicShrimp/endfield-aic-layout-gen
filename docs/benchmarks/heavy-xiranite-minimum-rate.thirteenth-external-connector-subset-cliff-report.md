# Heavy Xiranite External Connector Subset Cliff Report

## Question And Method

This checkpoint asks which smallest clean subset of the four Heavy Xiranite phase-zero external
requirements prevents Pumpkin from proving the primary used-area objective within five seconds.
Every non-empty subset was rebuilt from selected logical edges before external-requirement
partitioning. All 15 cases use the unchanged
`joint-shared-transport-layer-external-connectors-v1` formulation, a 12 by 12 caller ceiling, an
independent 5,000 ms release-mode budget, and one isolated process measured with macOS
`/usr/bin/time -l`.

No placement, rotation, port, connector template, coordinate, or objective value was fixed. Every
case writes JSON and self-contained HTML, including the two cases that returned no geometry.

The stable route-index legend is:

| Route | Role | Transport | Item | Rate |
| ---: | --- | --- | --- | ---: |
| 0 | IN | Pipe | `item-liquid-xiranite-poly` | 1/2 |
| 1 | IN | Belt | `item-xiranite-powder` | 1/2 |
| 2 | IN | Belt | `item-xiranite-powder` | 1/2 |
| 3 | OUT | Belt | `item-xiranite-enr-powder` | 1/10 |

## Five-Second Matrix

`Area proof` reports only the primary objective stage. A `feasible` final status can therefore
still have a proven area when the later transport-tile stage exhausted the remaining budget.

| Routes | Count | Status | First incumbent | Area proof | Objective `(area, tiles, turns)` | Variables | Log2 volume | Constraints | Incidences | Peak RSS |
| --- | ---: | --- | ---: | --- | --- | ---: | ---: | ---: | ---: | ---: |
| 0 | 1 | optimal | 58 ms | 309 ms, proven | (30, 1, 0) | 3,383 | 3,423.86 | 19,176 | 70,431 | 25.9 MiB |
| 1 | 1 | unknown | none | not reached | none | 4,441 | 4,486.50 | 23,628 | 81,729 | 60.6 MiB |
| 2 | 1 | unknown | none | not reached | none | 4,441 | 4,486.50 | 23,628 | 81,729 | 60.3 MiB |
| 3 | 1 | optimal | 195 ms | 416 ms, proven | (30, 1, 0) | 4,791 | 4,836.50 | 25,112 | 85,495 | 32.3 MiB |
| 0,1 | 2 | optimal | 55 ms | 1,372 ms, proven | (35, 2, 0) | 7,394 | 7,455.84 | 35,791 | 112,729 | 41.2 MiB |
| 0,2 | 2 | optimal | 55 ms | 1,369 ms, proven | (35, 2, 0) | 7,394 | 7,455.84 | 35,791 | 112,729 | 41.3 MiB |
| 0,3 | 2 | optimal | 301 ms | 1,402 ms, proven | (36, 2, 0) | 7,744 | 7,805.84 | 37,275 | 116,495 | 48.4 MiB |
| 1,2 | 2 | optimal | 54 ms | 737 ms, proven | (30, 2, 0) | 8,448 | 8,514.48 | 40,248 | 124,037 | 53.8 MiB |
| 1,3 | 2 | feasible | 24 ms | 1,212 ms, proven | (35, 2, 0) | 8,798 | 8,864.48 | 41,727 | 127,793 | 51.8 MiB |
| 2,3 | 2 | feasible | 25 ms | 1,208 ms, proven | (35, 2, 0) | 8,798 | 8,864.48 | 41,727 | 127,793 | 51.8 MiB |
| 0,1,2 | 3 | optimal | 44 ms | 2,598 ms, proven | (36, 3, 0) | 11,401 | 11,483.24 | 52,411 | 155,037 | 64.9 MiB |
| 0,1,3 | 3 | feasible | 13 ms | 2,847 ms, proven | (42, 3, 0) | 11,751 | 11,833.24 | 53,890 | 158,793 | 67.3 MiB |
| 0,2,3 | 3 | feasible | 13 ms | 2,939 ms, proven | (42, 3, 0) | 11,751 | 11,833.24 | 53,890 | 158,793 | 67.7 MiB |
| 1,2,3 | 3 | optimal | 53 ms | 1,003 ms, proven | (35, 3, 0) | 12,805 | 12,891.88 | 58,347 | 170,101 | 76.3 MiB |
| 0,1,2,3 | 4 | feasible | 47 ms | 5,000 ms, unproven | (42, 12, 2) | 15,758 | 15,860.37 | 70,510 | 201,101 | 70.9 MiB |

RSS and timing are single-run observations. Structural counts are deterministic for this revision.
All emitted geometry passed independent validation. Routes 1 and 2 returned `unknown`, not
infeasible, and their failure HTML retains complete solver evidence.

## First Cliff: One Mixed-Side Belt Input

The smallest failing composition is either single Xiranite Powder belt input, route 1 or route 2.
Both exact models are structurally identical and independently reproduce the same outcome: no
first incumbent in 5,000 ms. The single pipe input and single belt output both prove the complete
lexicographic optimum in less than half a second.

Raw model size does not explain this boundary. Compared with the tractable single belt output,
one failing belt input has 350 fewer variables, 1,484 fewer constraints, and 3,766 fewer factor
incidences. Both have the same endpoint domains: a five-value port selector and a 1,280-value
placement-port selector. Despite being smaller, the failing input consumed about 60 MiB RSS versus
32.3 MiB for the output.

The relevant game-data geometry differs:

| Endpoint kind | Compatible port sides |
| --- | --- |
| Pipe IN | one west port |
| Belt OUT | five north ports |
| Belt IN | four south ports and one east port |

The strongest remaining explanation is therefore search and propagation across the disjunction
between differently oriented input-port geometry, connector templates, and dynamic used bounds.
This is a hypothesis, not yet a root-cause proof.

The matrix is deliberately non-monotonic. Adding the second identical Powder input changes
`unknown` into a proven optimum: route set 1,2 finds its first incumbent in 54 ms and proves area in
737 ms even though it adds 4,007 variables, 16,620 constraints, and 42,308 incidences relative to
route 1 alone. The extra connector also adds a distinct-port collision constraint. That added
constraint may remove a large symmetric or weakly propagated region, but this matrix does not
separate that effect from mixed-side port geometry.

## Second Cliff: Full-Set Area Proof

Every three-requirement case proves the primary area objective within 2,939 ms. The complete
four-requirement model still finds a valid incumbent in 47 ms but cannot prove area 42 within the
five-second budget. This is an optimization-proof cliff, not a first-feasible cliff.

Compared with tractable route set 1,2,3, adding the pipe input to form the complete set adds 2,953
variables, 12,163 constraints, and 31,000 incidences. First-incumbent time remains effectively
unchanged, while primary proof time rises from 1,003 ms to more than 5,000 ms. Compared with route
set 0,1,2, adding the output adds 4,357 variables, 18,099 constraints, and 46,064 incidences and
changes a 2,598 ms area proof into an unproven area stage.

Later objectives form a separate, smaller cliff. Route sets 1,3; 2,3; 0,1,3; and 0,2,3 prove area
but exhaust the remaining budget while proving the physical transport-tile objective. Their
`feasible` status must not be mistaken for a primary-area failure.

## Dominant Model Family

The exact connector encoding is the dominant static family. A single Powder input creates 4,028
connector-family variables out of 4,441 total. The complete case creates 15,336 connector-family
variables out of 15,758 total, 62,913 connector-family constraints out of 70,510 total, and 158,766
connector-to-connector incidences out of 201,101 total.

These variables enumerate placement-port geometry, one of three templates, template-and-geometry
conjunctions, dynamic-bound predicates, and derived cell occupancy. The formulation successfully
removed free route grids, arcs, flows, branches, bridges, and crossings from phase zero, but its
finite exact connector selector remains a large Boolean expansion.

| Recorded family metric | Single Powder input | Complete set |
| --- | ---: | ---: |
| Connector Boolean variables | 4,027 | 15,332 |
| Connector integer variables | 1 | 4 |
| Connector variable domain | Boolean 2; template 3 | Boolean 2; template 3 |
| Port selector domain per Powder connector | 5 | 5 |
| Placement-port selector domain per Powder connector | 1,280 | 1,280 |
| Endpoint-geometry domain per connector | 576 | 576 |
| Connector constraints | 16,469 | 62,913 |
| Connector constraint terms/incidences | 41,572 | 158,766 |
| Connector-to-collision incidences | 144 | 576 |
| Connector-to-used-geometry incidences | 288 | 1,152 |

The recorder's total term count equals total factor incidence count for these linear and table
constraints, so the matrix incidence column also reports total model terms. Placement is coupled to
each connector through the factored placement choice, port selector, and endpoint-geometry key
rather than through a flattened placement-by-port Boolean tuple.

Static connector volume explains why the complete proof becomes expensive. It does not explain
the single-Powder anomaly, because the larger single-output connector model is easy and adding a
second Powder connector makes search easier. Port-side/template/bound propagation is therefore the
next local target, while the connector-family expansion remains the full-model scaling target.

## What This Rules Out

- Commodity routing networks, flow conservation, splitters, convergers, bridges, and crossings
  cannot cause this phase-zero cliff; every case has zero such state.
- Model construction is not the operational cliff. It ranges from 22 to 68 ms; solver search
  dominates elapsed time.
- External connector count, total variables, total constraints, total incidences, and RSS are not
  monotonic predictors of first-incumbent difficulty.
- A five-port endpoint domain alone is insufficient to explain failure. The five-port north-facing
  output is larger but tractable.
- Route identity or rate does not explain the Powder result. Routes 1 and 2 have the same item,
  rate, transport, port compatibility, structural counts, and independently reproduced outcome.
- The 12 by 12 search ceiling is controlled and identical across cases; no case treats unused
  capacity as blueprint geometry.

## Cumulative Improvements Before This Diagnosis

| Checkpoint | Improvement | Measured outcome at 12 by 12 phase zero |
| --- | --- | --- |
| Release-mode baseline | Removed debug-build distortion | All solver performance claims now use optimized Rust builds |
| Circulation permitted | Removed route-order proof state | Removed 432 variables, 1,584 constraints, and 4,752 incidences |
| Shared transport layers | Replaced one dense grid per commodity with one belt layer and one pipe layer | Variables -24.0%; constraints -22.0% |
| Factored endpoints | Separated placement and port state | Endpoint states -66.9%; placement-routing incidences -43.2%; isolated RSS -10.2% |
| External boundary connectors | Replaced free external routing with three exact solver-selected straight templates | First valid full-set phase-zero incumbent improved from none in 15 seconds to about 50 ms |
| Subset diagnosis instrumentation | Added clean external-edge subset rebuilding, isolated RSS, and per-case JSON/HTML | Exposed mixed-side single-input search failure and full-set area-proof cliff separately |

The cumulative changes preserve solver authority over placement, rotation, compatible port choice,
and the approved external template choice. No layout or routing heuristic was introduced.

## One Next Experiment

Run one explicitly `diagnostic_only` port-domain decomposition of the single Powder input. Compare
the faithful five-port case with south-only, east-only, and one representative south-port cases
under equal five-second budgets. These restricted cases exclude legal solutions and therefore may
not be used as production layouts, baselines, hints, or automatic reductions.

This one-axis matrix distinguishes the remaining explanations:

- if south-only and east-only solve but the faithful union does not, the mixed-side disjunction is
  the local cliff;
- if four south ports remain hard but one south port solves, same-side port multiplicity or
  symmetry is the local cliff; and
- if a one-port case remains hard, the connector-template/dynamic-bound encoding itself is already
  sufficient to cause the cliff.

Do not perform this experiment until the current report has been reviewed.

## Artifacts And Reproduction

- Contract: `docs/designs/external-connector-subset-cliff-diagnosis.md`
- Normalized matrix: `docs/benchmarks/heavy-xiranite-external-connector-subsets/summary.json`
- Per-case JSON, HTML, and `/usr/bin/time -l` records:
  `docs/benchmarks/heavy-xiranite-external-connector-subsets/`

One case can be reproduced with:

```bash
/usr/bin/time -l -o subset-1.time.txt \
  target/release/aic-cli research solve-first-phase-external-subset \
  --workload data/benchmarks/heavy-xiranite-minimum-rate.workload.json \
  --workspace-root . \
  --placement-request data/benchmarks/requests/placement.12x12.request.json \
  --route-index 1 \
  --time-limit-ms 5000 \
  --output subset-1.json \
  --visualization-output subset-1.html
```
