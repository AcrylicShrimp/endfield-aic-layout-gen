# Heavy Xiranite Minimum-Rate Phase 2 Final Cliff Breakdown

## Result

The three-facility Phase 2 first-witness cliff is not caused by facility count alone. Facility placement, port selection, and boundary-terminal selection all contribute, but the final dominant blocker is the shared routing core.

The routing core still requires several seconds after every facility placement and every terminal are fixed to a validated reference.

## Breakdown ladder

| Diagnostic | Solver freedom retained | Result |
| --- | --- | ---: |
| Fixed 12 by 12 dimensions | All placements, ports, terminals, routing | No witness in 30 s |
| Introduced-facility coordinate partition | Rotation, ports, other placements, terminals, routing | 0 witnesses across 64 one-second cases |
| Introduced-facility port partition at `(0,1)` | Rotation, other placements, terminals, routing | 0 witnesses across 125 one-second cases |
| Introduced-facility rotation partition | Other placements, their ports, terminals, routing | One 180-degree witness in 15.582 s |
| All three placements fixed | Ports, terminals, routing | First witness in 12.890 s |
| Placements and facility ports fixed | External terminals, routing | First witness in 11.144 s |
| Placements and all terminals fixed | Routing only | First witness in 6.636 s |

The 5-second reference-ablation run returned `unknown` for all three cases, including the routing-only case. The 30-second run found and validated all three witnesses.

## What each layer costs

The timings are not strictly additive because each case follows a different solver path, but they provide a useful localization:

| Decision layer removed from search | Observed first-witness reduction |
| --- | ---: |
| Facility placement | 14.956 s reference to 12.890 s: about 2.1 s |
| Facility port selection | 12.890 s to 11.144 s: about 1.7 s |
| External boundary-terminal selection | 11.144 s to 6.636 s: about 4.5 s |
| Shared routing core still remaining | 6.636 s |

Boundary terminals are a meaningful secondary cliff. They are not the final cliff: fixing all eight of them still leaves more than six seconds of routing search.

## Final cliff target

With placements and terminals fixed, the model still jointly chooses:

- shared belt and pipe route cells;
- per-arm item identity;
- directed route arcs;
- integer flow on arcs;
- flow conservation and capacity;
- splitter, converger, and bridge topology;
- facility/transport collision and same-layer crossing legality.

This block contains the next research target. The relevant model remains:

| Metric | Value |
| --- | ---: |
| Variables | 23,972 |
| Constraints | 82,648 with all reference terminals fixed |
| Factor-graph incidences | 285,128 |
| Placement-routing incidences | 83,454 |
| Route-cell variables | 576 |
| Route-arc variables | 1,056 |
| Flow variables | 1,056 |
| Route-arm variables | 2,304 |
| Arm-item variables | 1,440 |
| Branch-component variables | 2,304 |

The remaining search cost is therefore not explained by raw route-cell or arc counts alone. It comes from the coupled routing state and its constraints.

## Validated Phase 2 witness

The experiment also produced a validated 12 by 12 Phase 2 witness:

| Metric | Value |
| --- | ---: |
| Facility count | 3 |
| Commodity networks | 8 |
| Route requirements | 10 |
| Physical transport tiles | 103 in the reference witness |
| Route turns | 40 in the reference witness |
| Logistics components | 24 in the reference witness |

This proves a primary-area upper bound of 144. It does not close the primary optimum because 11 by 12 and 12 by 11 remain unresolved.

## Next interactive research step

Split the routing-only reference case internally, without changing production semantics:

1. fix only shared route-cell item ownership from the validated witness;
2. then fix route-arc activation/topology while leaving integer flow free;
3. then fix topology components while leaving route and flow decisions free;
4. compare which fixation collapses the 6.6-second first-witness time.

These must remain diagnostic ablations. The production solver must continue to decide placement and routing jointly until an exact reformulation or an explicitly approved architecture replaces it.

## Artifacts

- `heavy-xiranite-phase2-reference-ablation-5s/summary.json`
- `heavy-xiranite-phase2-reference-ablation-5s/summary.html`
- `heavy-xiranite-phase2-reference-ablation-30s/summary.json`
- `heavy-xiranite-phase2-reference-ablation-30s/summary.html`
- Per-case wireframes under both artifact directories.
