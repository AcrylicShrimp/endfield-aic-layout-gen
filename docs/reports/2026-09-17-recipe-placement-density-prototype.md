# Recipe placement density prototype

Date: 2026-09-17

## Question

Can the selected recipe facility instances be placed without changing their
semantic identities, while controlling packing by achieved facility-body density
rather than by a fixed gap setting?

## Prototype boundary

The prototype converts every facility node in the final instance-wiring report
to one physical block and retains an explicit semantic-node-to-block mapping.
External, target and surplus nodes are boundaries rather than facility bodies.
It then samples legal rotations and one seeded sequence pair. No recipe, source
cut, rate, facility instance or wiring obligation changes after placement starts.

The measured density is:

```text
sum of rotated facility body cells / facility-body bounding rectangle area
```

Margins and future route extents do not enter this measurement.

## Calibration panel

The bounded panel used five real target-item families, nominal facility counts
8, 16, 32, 59 and 96, and eight seeds. Every semantic result was placed even
when its achieved count differed from the nominal proposal. Of 600
profile-labelled rows, 459 semantic proposals matched their nominal count and
the largest nominal-to-achieved difference was 39. This confirms that placement
and corpus accounting must use the achieved count.

An initial fixed-gap panel showed that gaps are not portable profile definitions.
For example, the `[0, 4]` gap range entered the candidate balanced density band
in only 14 of 40 nominal 8-facility rows, while it entered that band in every
nominal 32-facility row. Sequence-pair shape and scale materially change the
resulting density.

The versioned first density policy is:

| Profile | Facility-body density |
| --- | ---: |
| `compact` | `[0.30, 1.00]` |
| `balanced` | `[0.13, 0.30)` |
| `spacious` | `[0.035, 0.12)` |

The intentional gap between spacious and balanced keeps the named regions
disjoint. Values outside all three bands remain observable misses rather than
being relabelled.

## Band-directed construction

For one seeded rotation and sequence-pair draw, the prototype applies one
uniform integer gap to every ordering relation. Body density decreases
monotonically as that gap increases. It uses exponential bracketing followed by
integer binary search to find the smallest gap inside the requested band. The
search reuses the catalogs and frozen blocks; it does not retry or alter the
semantic problem.

Across 200 rows per profile, the measured raw placement results were:

| Profile | In-band placements | Observed successful density | Selected gap |
| --- | ---: | ---: | ---: |
| `compact` | 184 / 200 (92%) | 0.306–0.727 | 0 |
| `balanced` | 200 / 200 (100%) | 0.205–0.299 | 0–4 |
| `spacious` | 200 / 200 (100%) | 0.093–0.120 | 3–10 |

All 16 misses were compact placements whose zero-gap sequence pair was still
below 0.30. They returned the structured `packing-target-missed` diagnostic.
They were not admitted under another profile and were not repaired by changing
the recipes or wiring.

Compact yield by nominal scale was 100%, 100%, 88%, 82% and 90% for 8, 16, 32,
59 and 96 respectively. This is sufficient for the first placement boundary,
but it also gives a concrete reason for the later bounded sequence-pair mutation
stage. The mutation stage should improve compact yield while retaining the same
semantic instance and density band.

## Integrated sample

The 59-facility xiranite sample at seed 2 retained all 59 semantic facility
nodes and all 114 wiring edges. With independently sampled gaps `[0, 4]`, its
body occupied 1,095 cells in a 65 by 86 body bounding rectangle, for density
0.195886. The report schema records the semantic-to-block mapping, sequence pair,
body metrics, chosen gap and canvas dimensions.

## Limits and next decision

This slice establishes frozen-block placement and packing control only. It does
not assign ports, convert logical wiring into physical networks, route, legalize
components or admit a complete layout. The density policy therefore remains
subject to a new version if routing evidence shows that one band is not
routable across the supported domain. The next comparison used identical frozen
semantic instances and placements for sequential and negotiated routing. Its
results are in
[`2026-09-17-recipe-routing-negotiation-prototype.md`](2026-09-17-recipe-routing-negotiation-prototype.md).
They show that the compact zero-gap samples in the bounded comparison can block
compatible port escape even though facility bodies do not overlap. The current
density policy therefore remains placement-only and cannot yet be used for
complete-layout admission.
