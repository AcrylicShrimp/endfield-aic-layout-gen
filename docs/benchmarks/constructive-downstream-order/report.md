# R1-order: finish internal downstream wiring before committing growth

## Outcome

Implemented `--downstream-closed-growth` as an explicit default-false experiment. Every accepted
growth state in this mode covers all original internal outgoing requirements of every placed
facility. Only unplaced upstream suppliers may remain internally. External input/output boundaries
remain pending until the existing final external-connection stage; this is not full external closure.

The order invariant works, including shared-producer groups and seed cycles, but does not yet
complete Heavy Xiranite. Keep the complete default and shared HTML unchanged. Nearby N=4 progresses
from 27 to 36 facilities, with different covered subgraphs, and costs more work/time. This is a
completion-progress observation, not a demonstrated complete-factory quality improvement.

## Contract and implementation

See the [pre-implementation contract](../../todos/2026-09-05.14-downstream-closed-growth.md).

- Request schema 1 adds default-false `downstream_closed_growth`; report schema 2 adds
  `growth_decisions`. Each entry identifies step, root, new group members, port variant and whether
  group routing succeeded. These are private attempts, not committed states; `steps` is authoritative.
- Starting from an unresolved supplier, transitively include its unplaced downstream dependencies.
  Stop traversal at already downstream-closed facilities. This includes SCCs and other consumers
  of shared producers without duplicating facility instances.
- Prefer smaller new groups, retaining the old pipe/upstream/ID order among equal sizes. This is
  a readiness/group-size heuristic, not an optimization proof. No spatial axis was added.
- Build each connected group privately with the unchanged incremental placement, port and routing
  functions. Score and candidate pools remain unchanged. Close all induced internal edges, then
  independently audit original coverage and internal outgoing closure before committing.
- A seed with outgoing internal obligations is itself private until its downstream group completes.
  On failed initialization there is no exposed final node, rather than a falsely closed seed.
- Failed branches consume the same per-growth placement/A* budget and preserve the previous closed
  incumbent. No automatic switch to the old order or external/global solver occurs.

The construction policy/design now explicitly allow these user-approved connected dependency groups.
They do not allow a global facility-only placement stage, rigid group geometry, or a larger canvas.

## Release measurements

The [initial matrix](../../../artifacts/benchmarks/constructive-downstream-order/matrix.json) has ten runs. The [three-round repeat](../../../artifacts/benchmarks/constructive-downstream-order/repeated.json) has 30 runs,
rotating variant order. Both use the same source plans/catalogs, seed, output mode, 256 growth steps,
256 placement trials and 4,096 A* calls per growth transaction. Per-connection limit remains 64.
Build and tests finish before timing; no compiler/test overlap. A 180-second per-run harness wall
guard was never reached. Repeated witnesses and search counters match within each variant.

Group size changes the unit of a transaction: identical numeric caps are NOT equal total work or
equal available work per facility. Actual calls/trials below distinguish that. No equal-time or
A*-expanded-state superiority is claimed. Repeated deterministic runs measure timing variability,
not independent layout samples. The harness captures stdout privately and writes detailed JSON files.

Heavy Xiranite, median of three planner measurements:

| Order / placement | Complete | Facilities | Internal / external covered | Area / tiles / turns | Planner s | A* calls | Placement trials |
| --- | --- | ---: | --- | --- | ---: | ---: | ---: |
| Old / coarse, preserved binary | Yes | 59 | 89 / 7 | 23,100 / 4,265 / 298 | 3.163 | 178 | 162 |
| Old / coarse, candidate disabled | Yes | 59 | 89 / 7 | 23,100 / 4,265 / 298 | 3.179 | 178 | 162 |
| Closed / coarse | No | 30 | 31 / 0 | 5,913 / 659 / 53 | 0.427 | 182 | 313 |
| Old / nearby N=4 | No | 27 | 36 / 0 | 1,953 / 914 / 75 | 4.329 | 487 | 435 |
| Closed / nearby N=4 | No | 36 | 41 / 0 | 2,700 / 1,040 / 84 | 12.830 | 1,266 | 598 |

Partial scores in this table describe diagnostic geometry only. The smaller closed/coarse runtime
is an early failure, not an acceleration of complete construction. Median end-to-end times including
file output are respectively 4.855, 4.871, 0.640, 4.511 and 13.065 seconds.

The disabled candidate matches the original final/intermediate witnesses and search statistics.
Its complete final-node hash remains
`acd24c232943a16a53455e6de1b6aea8d9bd95137ab5302568f5b15e9f2e6009`.

The small seed-cycle workload completes for all variants: coarse score stays 294 / 40 / 2 and
nearby score stays 182 / 24 / 2. Closed order does not change this fixture's final witness.

The harness directly checks every retained step/final node against the original internal edge ledger:
both closed-order configurations have zero uncovered internal-output occurrences. The old complete
constructor has 33 such occurrences across its intermediate snapshots, despite eventually completing
everything. These occurrences are not distinct facilities/requirements and not validation violations
under the old contract. External output obligations are deliberately excluded from this metric.

## Next bottleneck: confirmed reservation collision in coarse mode

Closed/coarse reaches 30 facilities (73 x 81). Its final failed transaction tries two-member groups,
exhausts 256 placement trials and records 144 route calls, all returning no path before a completed
lane bundle. No final future-boundary rejection candidate is recorded for this transaction.

The repeated failed connection is
`wiring-edge:21c5b34ce05a1a6145f4451f9000d03a9184f3301407bb9ecb356b9339b77043`:
an unplaced thickener supplies enriched moss powder to a placed furnace, at rate 1/2.
The furnace's selected west-facing `input-belt-0` has connection cell `(54,25)`.

That cell is also the actual connection cell (and reservation start) of a different placed grinder's south-facing input for
requirements `1770726fc972...` and `833307cbcd65...`. See the complete identities/geometry in
[coarse-reservations.json](../../../artifacts/benchmarks/constructive-downstream-order/coarse-reservations.json).

This directly explains the coarse endpoint obstruction under current policy:

1. `network/first.rs` marks three straight outward cells of other pending same-layer ports blocked.
2. It exempts ports matching either currently selected endpoint by facility/port identity. The
   other grinder is neither the new thickener source nor the furnace target, so it is not exempt.
3. `routing.rs::route_crossings` rejects a blocked goal before exploring the grid.

The target is already forbidden by unchanged incumbent reservations; relocating only the new
thickener cannot remove this obstruction. The aggregate `astar_searches` counter includes calls
that immediately reject endpoints; it is not the number of nontrivial graph searches.

The offline [reservation audit](reservations.mjs) finds two conflicted pending endpoints in this
coarse snapshot. It reads the reservation length explicitly (3, verified against the Rust policy
constant), ignores duplicate demands on the same port, separates transport layers, and reports
both endpoint and reserving-port identities. Two unit tests cover these semantics.

This is a constructor reservation-policy conflict, not game infeasibility. The existing per-demand
future-boundary screen does not prove joint compatibility of every reserved corridor. Do not fix
this by disabling independent validation or by declaring these straight corridors game rules.

## Nearby failure: narrower evidence, unresolved physical cause

Closed/nearby reaches 36 facilities (108 x 25), after 32 singleton commits and one three-facility
commit. The final three-member group fails under the placement cap; 190 route calls all return no
path, with zero completed lane bundles and no stored final `last_obstruction` candidate.

The repeatedly failed connection is
`wiring-edge:05e089285ba40fa7965113fad2d6e3c58db56ee47bade60deef31023d6c6a1e8`:
an unplaced moss planter supplies a grinder, at rate 1/4. The grinder is at `(93,16)`, rotation 180,
with a north-facing input connection at `(95,15)`. The reservation endpoint audit finds zero conflicts
in this retained snapshot. Therefore the confirmed coarse endpoint explanation cannot simply be
asserted for nearby mode. Candidate-specific blocked endpoints, crossing options and reachable
regions need a bounded route-failure witness before choosing a repair.

The remaining 23 boundaries include internal inputs and external inputs/outputs, not internal
outputs. Internal direction is now consistent, but neither geometrically aligned input frontiers
nor jointly routable future interfaces are guaranteed by graph order alone.

## Next decision and verification

Retain opt-in status. Next, align future-interface reservation/acceptance semantics with actual
route preconditions and retain a bounded failing-port-pair witness (endpoint blockers, reservation
owners, crossing/region evidence). Use the coarse conflict as a regression; investigate the nearby
failure separately. Spatial direction remains a proposal strategy, not a substitute for that check.
Joint reconstruction is still available as a later bounded repair, not implemented in this slice.

Verification: `cargo fmt --all`, `cargo check --workspace`, `cargo test --workspace` (501 pass,
two existing research ignores), release build, manual 40-run CLI matrix, Node syntax checks and two
offline reservation tests. The six new Rust tests cover dependency closure, graph-shape/determinism
and budget invariants, failed private groups/seeds, independent closure audit, default compatibility
and CLI flag/short-output behavior. Existing network and original-wiring audits remain in force.

Reproduce from the repository root, using fresh output paths:

```sh
AIC_ORDER_ROUNDS=3 node docs/benchmarks/constructive-downstream-order/run.mjs \
  /tmp/aic-downstream.kTgcr8/baseline-aic-cli target/release/aic-cli /tmp/aic-order-new-runs
node --test docs/benchmarks/constructive-downstream-order/reservations.test.mjs
node docs/benchmarks/constructive-downstream-order/reservations.mjs \
  /tmp/aic-downstream.kTgcr8/matrix/heavy-xiranite-0-closed-coarse.json \
  3 /tmp/aic-order-new-reservations.json
```

Large raw JSON/HTML reports remain in the temporary experiment directory; checked-in summaries
and hashes persist. If those temporary files expire, rebuild the baseline at `c7e01aa` separately
and rerun the harness. No existing remote result was replaced or website delivery changed.

## Requested visualization

The user additionally requested visualization of the issue. [render-failure.mjs](render-failure.mjs)
renders exact retained geometry around the two failed input cells, with facility footprints,
port-normal arrows, constructor reservations and existing transport geometry. It distinguishes the
confirmed coarse conflict from the unresolved nearby route failure; it does not draw an invented
failed candidate or imply that neighboring belts alone prove the nearby cause.

The existing Tailscale-only server now also serves:

- `http://100.114.24.86:8765/downstream-input-obstruction.html`: two-case zoomed diagnostic.
- `http://100.114.24.86:8765/closed-coarse-partial.html`: full 30-facility retained history.
- `http://100.114.24.86:8765/closed-nearby-4-partial.html`: full 36-facility retained history.

The original complete and 27-facility pages are unchanged. Diagnostic pages explicitly say partial
and link to the complete baseline. Verified HTTP response and screenshots, plus eight automated
browser checks covering both tabs at 360/736 px in light/dark appearance, correct selection and no
horizontal overflow. Generated serving files remain ignored under `target/shared-results`.

```sh
node docs/benchmarks/constructive-downstream-order/render-failure.mjs \
  /tmp/aic-downstream.kTgcr8/matrix/heavy-xiranite-0-closed-coarse.json \
  /tmp/aic-downstream.kTgcr8/matrix/heavy-xiranite-0-closed-nearby-4.json \
  /path/to/new-diagnostic.html
```

Keep the two full-history files beside the diagnostic when serving its relative links. The
regression requirement IDs choose these measured cases; catalog geometry still comes from reports.
