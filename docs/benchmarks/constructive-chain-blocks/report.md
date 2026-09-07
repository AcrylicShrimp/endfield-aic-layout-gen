# Straight-chain blocks and quantitative boundary demand

## Result

The full 59-facility graph contracts thirteen geometric belt edges into seven chains covering
twenty facilities. Including singletons, pass two places 46 blocks. All thirteen contracted
connections route straight under all three tested channel settings. No facility, logical edge,
recipe, rotation or catalog-relative port is lost. Shared suppliers and cycles are not contracted.

This is an isolated, user-approved placement-only/frozen-routing experiment, not a replacement for
connected transactional production construction. The best coverage is **85/89 internal
requirements**. Four internal requirements and all seven external requirements remain pending;
there is **no complete validated factory**. The previous historical 64/89 witness is preserved.

## Controlled comparison

Each row uses the same block-level heuristic and unchanged release routing engine, with either
chain contraction enabled or every facility treated as a singleton. The minimum channel is a
parameter, not a game rule. Actual channel allowance is the larger of that minimum and half the
block's boundary belt requirement count rounded up. It is a geometric heuristic, not a lane proof.
Grouping also changes the induced placement/routing order; this is an algorithm-level comparison,
not an order-held-constant causal attribution.

| Units | Minimum channel | Facility bounds | Facility area | Internal connected | Placement median ms | Routing ms |
| --- | ---: | --- | ---: | ---: | ---: | ---: |
| Singletons | 1 | 59 x 76 | 4,484 | 77/89 | 1,060.59 | 6,112.04 |
| Chains | 1 | 57 x 72 | 4,104 | 75/89 | 595.37 | 4,849.41 |
| Singletons | 3 | 70 x 108 | 7,560 | 81/89 | 1,186.68 | 5,975.59 |
| Chains | 3 | 64 x 109 | 6,976 | 84/89 | 683.61 | 3,296.08 |
| Singletons | 6 | 97 x 122 | 11,834 | 85/89 | 1,188.42 | 6,046.93 |
| Chains | 6 | 82 x 109 | 8,938 | 85/89 | 637.22 | 5,864.45 |

At minimum channel three, chains gain three connections and reduce facility area 7.72%. At six,
coverage ties and area is 24.47% smaller. At one, chaining loses two connections despite using less
area. This is evidence for useful process blocks, not universal dominance or monotonic improvement
with spacing. Compared with the old 52 x 79 facility proposal, high-coverage variants are larger.
Facility-only area is not the production score of a complete routed factory.

`matrix.json` preserves three sequential fresh-process placement samples per row, phase timings,
geometry hashes, frozen input/model/release binary hashes and one complete bounded routing run
per row. Timing excludes builds, tests and browser capture. Warm files; no concurrent test/build
load. Routing timings are single observations, not medians or confidence intervals. Earlier
exploratory measurements were not substituted for this frozen final matrix. All covered-route
validators pass, all coordinates remain fixed and all A* accounting stays within 64 per requirement.

## Placement method

An edge is contracted only for one belt successor at its source and one belt predecessor at its
target (including external belt branches), one belt demand between the pair, east/west candidate
ports and no directed return path through the full graph. This conservatively excludes cycles.
Select aligned candidate rows maximizing the number of aligned ports, then minimize center offset.
Capacity is not assumed from that alignment. Preserve internal requirements and pipe side ports.

Grow from the target block, prioritizing the most already connected pending block. Score finite
rectangle-edge and port-alignment events in both axes. After all blocks exist, two deterministic
coordinate-descent sweeps consider all incident edges, accepting strict local score improvements.
This is not recursive subtree packing; shared facilities have exactly one owner. Scores prioritize
backward belt edges, north-pipe preference violations, endpoint-distance sum, maximum endpoint
distance, then facility area. These are diagnostic placement priorities, not the production
complete-layout objective, and not a global optimization claim. Width/height are measured outputs.

The selected labels remain those of the frozen proposal. `placement_index` follows new block growth,
with downstream members first inside each block. Concrete routing still follows placement order,
nearest connected requirement and nearest port combinations; this experiment does not change it.

## Visually verified next obstruction

The six-cell-channel result first fails **F22 -> F16** after 33 retained connections. The browser
snapshot shows previous belts running down F16's input face. A read-only audit of that exact state
confirms five of six input connection cells occupied by the different `item-plant-moss-powder-3`
network. Only `input-belt-1` remains free. The network covers F23 -> F16/F17 and F26 -> F18/F19;
the audit records network membership, not a unique path owner for each shared cell.

F22 supplies `item-plant-moss-powder-2` at 1 item/s. Runtime belt capacity is 1 item per 2 seconds,
so at least two input lanes are required. F22 has three free output connection cells, but F16 has
only one free input candidate. The probe tries three source choices against that one target and
fails without a budget exhaustion. Retaining merely one future port is insufficient here. This
identifies a capacity/accessibility problem in the retained state, not global infeasibility.

For the three-cell-channel variant, the first F46 -> F38 failure exhausts 64 calls, with 27
last-port preservation rejections. The actual screenshot shows a seed-return connection around
several neighboring facilities. That bounded failure does not prove this placement impossible.

`first-obstruction.json` stores the six-cell snapshot's port occupancy, domains, demand, runtime
capacity, minimum lane count and trial trace. Published screenshots were personally inspected,
not inferred solely from route counts.

## User-requested input/output mapping

`interface-map.json` contains 83 ordered block-pair/commodity groups including external boundaries,
131 facility/direction/commodity terminal groups, and per-facility input/output port-count budgets.
Rates are added with exact rational arithmetic. Different commodities are never pooled into the
same lane budget; internal-chain and external obligations both count. No necessary capacity-count
deficit exists against catalog port counts in this proposal, but geometry and simultaneous
assignment can still fail, as F16 demonstrates.

| Logical relation | Required rate (items/s) | Minimum compatible lanes |
| --- | ---: | ---: |
| F30 + F31 -> F22, same item | 1/4 + 1/4 = 1/2 | 1 after a legal merge; 2 separate incoming branches |
| F22 -> F16 | 1 | 2 |
| F23 -> F16, different item | 1/2 | 1 |

Thus F16 needs at least **three input ports**, two for one commodity and one for the other, despite
having only two supplier facilities. Its catalog provides six input ports. F30/F31 also supply
F44, which returns seeds to both: the full topology is not always N-to-one. Pair counts alone do not
determine physical port counts. Aggregate trunk counts are lower bounds requiring legal compatible
merging/splitting, terminal capacity and routing; they are not preassigned port IDs or route proofs.

Next research slice: use these commodity-specific boundary budgets to select/reserve simultaneous
input/output port subsets and accessible approaches before routing other demands through their
faces. Test this on F22/F23 -> F16 and shared seed cycles. Do not replace the current router with a
global solver, silently widen gaps, or report an allocated port count as a validated connection.

## Verification and reproduction

- 27 Node tests pass, including chains, shared suppliers, cycles, external and side-pipe obligations,
  lane arithmetic, channel clearance, deterministic geometry and explicit file-output CLI behavior.
- `cargo check --workspace` and `cargo test --workspace` pass: 519 Rust tests, two existing ignores.
  The earlier snapshot milestone changed Rust; this block/interface slice does not.
- Runtime catalog hashes and every projected port are verified by the placement CLI.
- Actual browser overview, shared-group detail and first-failure pictures inspected. Six viewport/
  theme checks pass per page (1400/736/360 px, light/dark). Existing baseline pages remain untouched.
- Reviewed against AGENTS and `docs/designs/2026-09-02.00-constructive-layout-planner.md`: experimental
  offline CLI only, no production constructor, game data or exact solver changes.

```sh
node docs/benchmarks/constructive-chain-blocks/run.mjs NEW_MATRIX_DIRECTORY
node docs/benchmarks/constructive-chain-blocks/map.mjs \
  --placement artifacts/benchmarks/constructive-chain-blocks/gap-6-placement.json \
  --items data/game/normalized/items.json --transports data/game/normalized/transports.json \
  --report-output NEW_INTERFACE_MAP_JSON
node docs/benchmarks/constructive-chain-blocks/obstruction.mjs \
  artifacts/benchmarks/constructive-chain-blocks/gap-6-placement.json \
  artifacts/benchmarks/constructive-chain-blocks/gap-6-routing.json \
  data/game/normalized/transports.json NEW_OBSTRUCTION_JSON
```

Three chain placement/routing witnesses are retained here. Singleton samples can be regenerated by
the matrix runner; their exact metrics/hashes remain in `matrix.json`. Existing tailnet pages:
`http://100.114.24.86:8765/chain-blocks.html` and `http://100.114.24.86:8765/block-routing.html`.
