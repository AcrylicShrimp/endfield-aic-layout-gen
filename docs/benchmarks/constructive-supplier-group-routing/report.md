# Routing the port-centered supplier placement

## Result

The frozen 59-facility proposal connects **58/89 internal requirements**, including both pipes.
Thirty-one internal and seven external requirements remain pending. Covered routes independently
validate; this is not a complete factory. No facility moves relative to the placement input.

| Metric | Previous ordered routing | New supplier-group placement |
| --- | ---: | ---: |
| Connected internal requirements | 55/89 | 58/89 |
| Straight / other successes | 30 / 25 | 38 / 20 |
| Probe median, three release runs | 3,247.98 ms | 3,020.23 ms |
| A* calls | 640 | 614 |
| Future-domain rejections | 405 | 365 |
| Invalid-witness rejections | 0 | 0 |
| Summed network cells | 528 | 332 |

The identical release binary and budgets are used, so the changed input is the placement proposal.
The connected requirement sets differ: lower tile count is not a same-coverage optimum comparison.
The new used facility/network envelope is 54 x 82, starting at (-2,-2) in placement coordinates.
The 84 x 111 search canvas includes 16-cell padding and is a diagnostic resource envelope, not a
game limit or the factory footprint. Summed network cells may count crossing cells more than once.

## F14/F15 -> F09

Both motivating requirements connect on their first port pair, classified as straight:

- F14 output-belt-1 -> F09 input-belt-0.
- F15 output-belt-0 -> F09 input-belt-3.

Each pair has coincident connection cells and requires one physical belt cell between the two
facility rectangles. Each uses one search call and has zero witness/future-domain rejections.
The new placement removes the previous F15 -> F09 first failure without changing the router.

## First remaining failure: a multi-lane probe limitation

The first unresolved requirement is **F22 -> F16**, at placement index 22. It tries eighteen port
pairs and eighteen A* calls, including three zero-distance pairs. Neither witness validation nor
future-domain preservation rejects a completed candidate: both rejection counters are zero.

The requirement rate is **1 item/s**. The runtime transport catalog specifies **one belt item per
2,000 ms**, or **0.5 item/s**, so this requirement needs at least two belt lanes. The diagnostic
restricts both endpoint domains to one selected port each before invoking `network::connect_first`.
The shared engine supports recursive lane construction, but those restricted domains leave no
second available port after the first full-capacity lane. The entire connection attempt rolls back.

This is an implementation limitation of the fixed-pair diagnostic, not evidence that these nearby
facilities cannot be connected. Source inspection confirms the restriction in `fixed_probe.rs` and
remaining-rate recursion in `network/first.rs`. The existing regression
`attempted_port_pairs_are_distance_sorted_not_catalog_cross_product_order` deliberately exceeds
single-port capacity and reproduces this limitation; its focused rerun passes.

Six internal requirements exceed one belt lane and all six remain unresolved:
F22 -> F16, F24 -> F17, F25 -> F18, F27 -> F19, F41 -> F28 and F43 -> F29.
Do not attribute the other 25 failures to this one cause. Future-domain rejections still occur
elsewhere, and geometrically close ports do not prove simultaneous route feasibility.

The next implementation priority is **nearest port-pair selection per lane inside one atomic
multi-lane requirement transaction**, retaining unused compatible pairs for the remaining demand.
Validate and preserve future domains only on an accepted complete requirement bundle. Keep the
existing placement order, connection budget, capacity semantics and no-partial-commit invariant.
Do not reduce requested rates, inflate catalog belt capacity or relocate this group to hide the bug.
This run intentionally preserves the current baseline; no router change is included.

## Measurement, verification and publication

Three fresh release processes take 3,015.20 / 3,020.23 / 3,029.89 ms inside the probe, with CLI
wall times 3,055.98 / 3,059.90 / 3,068.88 ms. Warm files, no concurrent build/test load; build,
placement, catalog IO and report serialization are excluded from probe time. The binary hash
matches the previous ordered-routing measurement exactly. All three final witnesses hash to
`7ee7f1148ca1aca43865d3bbd78fe4737b4371d83e0c66da03a56ed18e89ea55`.

`routing.json` preserves run zero, `timing.json` every measurement and binary hash, and
`summary.json` counters, labeled failures and F14/F15 traces. All relative coordinates, rotations,
placement/neighbor ordering, port trial distances and per-connection 64-call budgets are audited.
Initial domains are nonempty; all accepted routes pass the existing independent validator.

No source behavior changed; no release rebuild or redundant full-workspace test rerun was needed.
The focused multi-port capacity regression passes against the already-verified source revision.
Browser checks pass at 1400/736/360 px in both themes: 58/89 status, 31 failures plus overview,
ports, failure selection, group inspection and no horizontal page overflow. Actual overview,
F14/F15 success and F22 failure screenshots were inspected. The red dashed line is the selected
unresolved logical requirement over the final retained witness, not a rejected physical route.

The existing tailnet server publishes `http://100.114.24.86:8765/supplier-group-routing.html`.
Browser-rendered images are saved under `target/shared-results/supplier-group-routing.png`,
`supplier-group-routing-detail.png` and `supplier-group-routing-failure.png`. Earlier pages remain.

## Reproduction and architecture boundary

```sh
target/release/aic-fixed-routing-probe \
  --placement docs/benchmarks/constructive-supplier-groups/heavy.json \
  --facilities data/game/normalized/facilities.json \
  --items data/game/normalized/items.json \
  --transports data/game/normalized/transports.json \
  --components data/game/normalized/logistics-components.json \
  --report-output NEW_ROUTING_JSON
node docs/benchmarks/constructive-placement-only/route-view.mjs \
  docs/benchmarks/constructive-supplier-groups/heavy.json NEW_ROUTING_JSON NEW_HTML
```

The default 16-cell padding and 64-call connection limit are unchanged. Output paths must be new;
stdout is a short completion only. Exit zero means the diagnostic ran, not factory completion.
Compared against AGENTS and `docs/designs/2026-09-02.00-constructive-layout-planner.md`, this remains
the user's isolated frozen-placement exception. Production connected construction, external
routing semantics, exact research tooling and runtime game data are unchanged.
