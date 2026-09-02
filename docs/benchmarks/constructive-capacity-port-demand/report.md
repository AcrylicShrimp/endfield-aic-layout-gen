# Constructive Capacity-Grouped Port Demand Report

## Purpose

This slice measures how many facility ports are actually required by item identity, direction, and
runtime belt/pipe capacity. It is observational: the existing constructive candidate validity rule
is not changed yet.

For one `(facility, direction, item, transport)` group:

```text
required ports = ceil(total group rate / catalog line capacity)
```

Required counts for distinct items are added within the same facility, direction, and transport
scope. Unused catalog ports have no demand and need not remain accessible.

## Heavy Xiranite Six-Step Result

| Metric | Result |
| --- | ---: |
| Facilities placed | 16 |
| Routed requirements | 15 |
| Used bounds | 42x11 |
| Unresolved edge boundaries | 14 |
| Capacity groups | 14 |
| Ports implied by one-per-edge | 14 |
| Ports required by capacity | 16 |
| One-per-edge deficit | 2 |
| Facility scopes lacking enough current port choices | 0 |
| Already routed networks over line capacity | 1 |
| Warm release time | 0.85 s |

The fourteen current boundary edges happen to belong to fourteen distinct groups, so same-item
aggregation does not reduce the count at this frontier. Two belt input groups instead expose the
opposite error: each has a total rate of `1 item/s`, but one belt carries only `1/2 item/s`. Each
group therefore needs two ports and two lines, while the old edge model implies only one.

Every affected facility scope currently has enough distinct exposed port choices. The geometry is
not immediately incapable of serving the demand, but the current boundary contract neither selects
nor reserves the second required port.

## Routed-Capacity Finding

The same audit finds one already embedded route that violates catalog capacity:

| Item | Transport | Reported rate | Line capacity | Required parallel lines |
| --- | --- | ---: | ---: | ---: |
| `item-plant-moss-powder-2` | belt | 1 item/s | 1/2 item/s | 2 |

The outer automatic-assembly report remains `success: true` because this slice deliberately leaves
the interim constructor and validator unchanged. Its nested `port_demand_analysis.success` is
`false` and carries the stable `constructive-routed-network-over-capacity` diagnostic. The current
HTML is therefore a useful geometry witness, not a capacity-valid blueprint.

## Conclusion

The next correctness slice is not merely an optimization. Constructive preparation must split any
logical rate above one line into the minimum number of positive capacity-bounded lanes before port
selection and route embedding. Same-item lanes may subsequently share trunks, split, and converge,
but every selected facility port and physical line must remain within catalog capacity.

## Reproduction

```bash
target/release/aic-cli layouts auto-assemble-process-modules \
  --recipes data/game/normalized/recipes.json \
  --source-plan data/examples/source-plan.game-heavy-xiranite-forge.request.json \
  --facility-catalog data/game/normalized/facilities.json \
  --item-catalog data/game/normalized/items.json \
  --transport-catalog data/game/normalized/transports.json \
  --target-instance facility-instance:recipe-occurrence:/target:0 \
  --max-steps 6 \
  --visualization-output docs/benchmarks/constructive-capacity-port-demand/heavy-xiranite.html \
  --report-output docs/benchmarks/constructive-capacity-port-demand/report.json \
  --localization-catalog data/game/normalized/localization.ko-KR.json
```
