# Constructive Growth Cliff Diagnosis

## Question

Why does capacity-valid Heavy Xiranite construction stop after eleven accepted automatic growth
steps even though the planner is allowed to grow without a W/H limit?

This diagnostic records every candidate rejected at the exhausted frontier, including the failed
stage, stable diagnostics, and composition search counters. Automatic assembly report schema
version 4 adds `exhausted_candidate_failures` for this purpose. Composition schema version 4 splits
post-route rejection counters by semantic rule.

## Result

The release-mode run stops after eleven accepted steps:

| Metric | Result |
| --- | ---: |
| Wall time | 54.46 s |
| Last valid facilities | 28 |
| Last valid routed networks | 27 |
| Last valid bounds | 68x16 |
| Unresolved frontier requirements | 12 |
| Generated candidates | 8 |
| Module-construction failures | 1 |
| Existing-member overlap failures | 1 |
| Composition failures | 6 |
| Valid composition candidates | 0 |

One module is rejected because its remaining capacity-grouped port demand exceeds its exposed port
choices. One candidate contains a facility already present in the immutable target node. The other
six enter whole-node composition.

## Composition Breakdown

| Event | Count |
| --- | ---: |
| Rigid placements considered | 41,502 |
| Facility collisions rejected | 28,244 |
| Port pairs considered | 592,269 |
| A* calls | 505,325 |
| A* failures | 216,173 |
| Completed lane bundles rejected by boundary rule | 186,951 |
| Completed lane bundles rejected by grouped capacity | 0 |
| Geometry validation rejections | 0 |
| Valid candidates | 0 |

The remaining 102,201 successful A* calls belong to incomplete multi-lane branches: the first lane
was found, but no complete set of all required lanes was produced. They never reached composite
validation.

## Root Cause

The immediate cliff is the mismatch between two boundary contracts:

- the new demand audit groups compatible requirements by facility, direction, item, and transport
  capacity;
- composition still rejects a candidate as soon as any single logical boundary edge has zero
  unused facility-port choices.

All 186,951 complete route bundles die in that older per-edge check before the grouped capacity
check runs. This is especially restrictive for same-commodity fan-out: several logical edges may
legally share one port and physical trunk while capacity remains available, but the current DTO can
only describe unused facility ports. It cannot describe attachment to an already synthesized
commodity network or the residual capacity of that network.

The multi-lane implementation exposes a second, smaller completeness weakness. It asks A* for only
one shortest path for each lane endpoint pair. A shortest first lane can block the second lane even
when a different first path would permit both. This affects incomplete bundle branches, but it is
not the rule that rejects the 186,951 fully routed bundles.

## Recommended Next Slice

Do not merely delete the per-edge boundary check. That would allow an abstractly capacity-valid
state which the current constructor may still be unable to extend. First add a boundary attachment
choice that can reference either:

1. an unused compatible facility port, or
2. an existing compatible commodity network with sufficient residual capacity.

Then synthesize one logical network per item and transport kind, extend its trunk/tree when a new
requirement is attached, and make grouped residual capacity the acceptance rule. Re-run this exact
frontier after that narrow cutover. Bounded rollback remains the recovery mechanism if the new
network-aware frontier still exhausts all candidates.

## Reproduction

```bash
target/release/aic-cli layouts auto-assemble-process-modules \
  --recipes data/game/normalized/recipes.json \
  --source-plan data/examples/source-plan.game-heavy-xiranite-forge.request.json \
  --facility-catalog data/game/normalized/facilities.json \
  --item-catalog data/game/normalized/items.json \
  --transport-catalog data/game/normalized/transports.json \
  --target-instance 'facility-instance:recipe-occurrence:/target:0' \
  --max-steps 12 \
  --visualization-output docs/benchmarks/constructive-growth-cliff-diagnosis/heavy-xiranite.html \
  --report-output docs/benchmarks/constructive-growth-cliff-diagnosis/report.json \
  --localization-catalog data/game/normalized/localization.ko-KR.json
```
