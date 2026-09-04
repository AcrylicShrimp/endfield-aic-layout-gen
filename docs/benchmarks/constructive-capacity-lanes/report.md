# Constructive Capacity-Bounded Lane Report

## Purpose

This slice changes transport capacity from an observational diagnostic into a constructive routing
contract. A logical requirement whose rate exceeds one runtime belt or pipe line is split into the
minimum number of positive, capacity-bounded lanes before port assignment and A* routing.

For a requirement rate `r` and catalog line capacity `c`:

```text
lane count = ceil(r / c)
```

The final lane may carry less than `c`. Every lane selects a distinct source port and target port,
and same-layer lane paths are physically disjoint.

## Review Corrections

The inherited uncommitted implementation compiled, but review found three correctness gaps:

- a shared test catalog accidentally changed an existing single-line fixture into a two-line case;
- composition did not re-check grouped future port capacity after consuming multiple ports;
- automatic assembly could still report `success: true` when its final capacity audit failed.

The slice corrects all three before accepting the implementation. It also makes the transport
catalog an explicit input to every constructive command that can create routed geometry.

## Controlled Capacity Result

The previously invalid Heavy Xiranite requirement carries `1 item/s` of
`item-plant-moss-powder-2`. Runtime belt capacity is `1/2 item/s`, so the constructive process
module must create two physical lines.

| Metric | Result |
| --- | ---: |
| Facilities | 2 |
| Used bounds | 6x7 |
| Logical requirements | 1 |
| Physical belt lanes | 2 |
| Lane rates | 1/2 + 1/2 item/s |
| Distinct source ports | 2 |
| Distinct target ports | 2 |
| Occupied belt tiles | 4 |
| Wall time | 0.01 s |

The two routes use `output-belt-1 -> input-belt-0` and
`output-belt-2 -> input-belt-1`. Their physical lengths are three and one tiles respectively.

## Heavy Xiranite Six-Step Regression

| Metric | Before capacity construction | After capacity construction |
| --- | ---: | ---: |
| Successful partial construction | yes | yes |
| Facilities | 16 | 16 |
| Used bounds | 42x11 | 42x11 |
| Routed networks | 15 | 15 |
| Over-capacity routed networks | 1 | 0 |
| Capacity audit success | no | yes |
| Warm release wall time | 0.85 s | 1.52 s |

The candidate order changed, so the two six-step results do not cover exactly the same partial
production subgraph. The geometry and timing columns are regression evidence, not an
apples-to-apples optimizer benchmark. The controlled module above is the direct proof that the
offending `1 item/s` edge is now represented as two legal lanes.

## Current Limits And Next Blocker

The implementation still creates independent point-to-point lanes. Compatible same-item demands
are not yet synthesized into shared trunks or trees. The lane enumerator also materializes all
valid lane bundles for one rigid placement before scoring them; this is safe for the currently
observed two-lane case but can become an avoidable memory and runtime multiplier at higher lane
counts.

A separate fifteen-step probe reached eleven accepted growth steps, 28 facilities, `68x16`, and 27
capacity-valid networks before the greedy immutable composition prefix exhausted every currently
generated module candidate. This is a constructive dead end, not an infeasibility proof. The next
slice should diagnose that growth step and determine whether bounded rollback, shared commodity
network synthesis, or streaming lane-bundle scoring is the smallest effective intervention.

## Reproduction

```bash
target/release/aic-cli layouts construct-process-module \
  --recipes data/game/normalized/recipes.json \
  --source-plan data/examples/source-plan.game-heavy-xiranite-forge.request.json \
  --facility-catalog data/game/normalized/facilities.json \
  --item-catalog data/game/normalized/items.json \
  --transport-catalog data/game/normalized/transports.json \
  --root-instance 'facility-instance:recipe-occurrence:/target/recipe:xiranite-oven-xiranite-enr-powder-1/input:item-xiranite-powder/recipe:xiranite-oven-xiranite-powder-1/input:item-carbon-enr/recipe:furnance-carbon-enr-1/input:item-carbon-enr-powder/recipe:furnance-carbon-enr-powder-2/input:item-plant-moss-enr-powder-2:3' \
  --internal-item item-plant-moss-powder-2 \
  --visualization-output docs/benchmarks/constructive-capacity-lanes/capacity-module.html \
  --localization-catalog data/game/normalized/localization.ko-KR.json
```

The six-step automatic regression uses the same inputs with
`layouts auto-assemble-process-modules`, the final target instance, and `--max-steps 6`.
