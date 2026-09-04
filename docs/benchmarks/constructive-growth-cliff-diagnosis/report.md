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

## Plain-Language Interpretation

The current constructor treats two logical consumers of the same item as if each one always needs
its own producer port and its own point-to-point route:

```text
producer OUT-0 ------> consumer B
producer OUT-1? -----> consumer C
```

The accepted network semantics allow one capacity-safe trunk to branch through an explicit
catalog-valid splitter instead:

```text
producer OUT-0 ---- [splitter] ----> consumer B
                         |
                         +----------> consumer C
```

The consumers still require their own input terminals. What can be shared is the compatible
producer port and the common belt or pipe segment. A shared segment is legal only when the sum of
all rates using it does not exceed the runtime line capacity. For example, a `1/2 item/s` belt that
already carries `1/5 item/s` has `3/10 item/s` residual capacity on that segment.

Today a boundary requirement can name only unused facility ports. It cannot name an existing
same-item network and the capacity-safe places where a catalog-valid splitter or converger could
join it. An ordinary line cell or facility terminal cannot branch by itself. Therefore the
post-route guard rejects a candidate when no separate unused port remains, even if a shared trunk
and explicit branch component could be legal. The 186,951 rejected complete lane bundles expose
this representation gap. They are not themselves proof of complete valid factories because
residual capacity, branch-component selection, and a realizable branch are not represented yet.

The next slice must expand the boundary alternatives rather than simply remove the guard. Its full
contract, invariants, controlled fixtures, and reconstruction map are recorded in
`docs/designs/2026-09-05.00-network-aware-constructive-frontier.md`.

The multi-lane implementation exposes a second, smaller completeness weakness. It asks A* for only
one shortest path for each lane endpoint pair. A shortest first lane can block the second lane even
when a different first path would permit both. This affects incomplete bundle branches, but it is
not the rule that rejects the 186,951 fully routed bundles.

## Recommended Next Slice

Do not merely delete the per-edge boundary check. That would allow an abstractly capacity-valid
state which the current constructor may still be unable to extend. First add a boundary attachment
choice that can reference either:

1. an unused compatible facility port, or
2. an existing compatible commodity network with sufficient residual capacity and a
   catalog-valid splitter/converger attachment anchor.

The constructive CLI must load the external logistics-component catalog during that cutover. Then
synthesize one logical network per item and transport kind, extend its trunk/tree when a new
requirement is attached, and make explicit component topology plus grouped residual capacity the
acceptance rules. Re-run this exact frontier after that narrow cutover. Bounded rollback remains
the recovery mechanism if the new network-aware frontier still exhausts all candidates.

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
