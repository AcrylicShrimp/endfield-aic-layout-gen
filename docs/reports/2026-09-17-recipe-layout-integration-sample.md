# Recipe selector to physical layout integration sample

Date: 2026-09-17

## Question

Can the source-tree selector recreate a problem at the scale of the original
`item-xiranite-enr-powder` benchmark, and what spatial results does the current
end-to-end path produce across several facility counts?

The remembered benchmark size was 59 facility instances and 96 logical wiring
requirements. The 96 value was the requirement count, not the facility count.

## Recipe selection

The selector does not synthesize recipes. It reads the normalized 305-recipe
game catalog, fixes a requested target item, and uses the seed to vary producer,
rate-start and equal-count subtree choices. Its output is therefore a seeded
selection and combination of real recipes with explicit external cuts.

For `item-xiranite-enr-powder`, requested count 59, depth four and 64 starts, all
ten seeds 0 through 9 produced an exact 59-facility plan. Selection took
0.859--11.460 ms with a 1.375 ms median. The plans contained 4--13 active recipe
occurrences and no DP truncation.

Seed 2 produced 59 facility nodes, seven external-input nodes, three surplus
nodes and one target node. Its validated wiring contained 114 edges: 58 internal
recipe flows, 38 external inputs, 15 surplus outputs and three target outputs.
This is larger by edge count than the historical 59-facility/96-requirement
benchmark, although it is a different valid production plan and rate.

## Physical integration

The selected source plan was passed unchanged through contextual throughput,
facility expansion, instance wiring and the existing constructive production
builder. The seed-2 59-facility case completed in 434 ms with all 58 internal and
56 boundary requirements resolved. Its final geometry contains 19 commodity
networks and 108 logistics components in a 131 by 222 bounding box. The 29,082
cell box contains 4,903 unique facility or transport cells, for 16.9% occupancy.

This successful integration is evidence that the selector output is directly
usable by the established physical pipeline. It does not establish that the
current first-feasible constructive placement has the packing control or raw
yield required by the accepted dataset-generator design.

## Seeded size and area sample

The controlled panel used the same target item, counts 8, 16, 32, 59 and 96,
seeds 0 through 4, depth four, 64 selector starts and the same bounded
constructive settings. Semantic selection was exact in all 25 cases. Physical
construction completed 11 of 25 cases; an exhausted case remains a bounded
failure and its partial geometry is not counted as a completed sample.

| Facilities | Complete | Completed bounding area | Median completed occupancy |
| ---: | ---: | ---: | ---: |
| 8 | 2 / 5 | 2,501--3,498 | 15.4% |
| 16 | 1 / 5 | 9,215 | 12.3% |
| 32 | 1 / 5 | 17,484 | 11.3% |
| 59 | 5 / 5 | 27,336--39,917 | 19.8% |
| 96 | 2 / 5 | 44,820--71,655 | 19.5% |

The sample exposes two current properties. Facility-count control is reliable in
the semantic selector, while physical raw yield depends strongly on the selected
topology rather than monotonically on count. The completed layouts are also
sparse: median occupancy is 11.3--19.8% in these cells. This is consistent with
the constructive builder's completion-first role; it has no requested packing
band and does not perform the placement search planned for the next physical
slice.

## Reproduction

Build the release commands, run `aic-recipe-leaf-probe` with target
`item-xiranite-enr-powder`, materialize its reported selections as a
`RecipeSourcePlanRequest`, and pass that request to:

```sh
target/release/aic-cli layouts construct-production \
  --recipes data/game/normalized/recipes.json \
  --source-plan SOURCE_PLAN.json \
  --facility-catalog data/game/normalized/facilities.json \
  --item-catalog data/game/normalized/items.json \
  --transport-catalog data/game/normalized/transports.json \
  --logistics-component-catalog data/game/normalized/logistics-components.json \
  --target-instance 'facility-instance:recipe-occurrence:/target:0' \
  --max-steps 256 --max-placement-attempts 256 \
  --max-route-searches 8192 --analysis-workers 4 \
  --nearby-candidates 0 --final-json
```

Compact local summaries and geometry remain under ignored `target/`; regenerable
full construction histories are not retained in Git.
