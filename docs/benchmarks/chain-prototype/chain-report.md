# Milestone 1: generated straight and bent chains

Status: complete for the simple-chain contract. The full research goal remains active.
No production generator changed. Cycles, branches, complete external factories, rollback,
completion-aware evaluation, and full matched-budget comparisons remain later milestones.

## Implemented behavior

`run.mjs` is a separate experimental CLI. Its request contains recipes, facility identities,
utilizations, internal requirements, declared external obligations and proposal/work controls.
It does not accept coordinates, rotations, selected ports, route corners, or a final canvas.
`input.mjs` verifies the exact recipe ledger and derives the unique sink-first chain order.
`geometry.mjs` generates relative straight/left/right approaches, catalog rotations, centered
and nearby port choices, and requested gap alternatives. `construct.mjs` grows and routes one
new upstream facility at a time. It reuses the dedicated A* routine and neutral geometry/rate/
score utilities, but not an existing constructor, screenshot transcriber or exact solver.

Every seed and candidate is independently checked by the Rust witness probe and the graph's
expected prefix ledger. The next facility is never introduced before selecting a valid routed
parent. Each family retains its own accepted history. Canonical translation changes only the
coordinate origin; tests verify that older facilities, ports, directed routes and rates remain
unchanged relative to each other. There is no global placement-only phase.

The family labels describe turns while traversing upstream from the sink. The supplier's
material-flow direction is opposite to that traversal. Configured gaps are retained in each
proposal, including gap 2; this milestone does not compact away deliberate spacing.

## Controlled results

The requests are chain fragments with explicit external cuts, not reduced substitutes for the
three full factories required by the goal. In particular, `iron-chain.json` represents only
one iron supplier chain and its thickener; the other iron input and plant-powder input are
declared external cuts for this milestone. Do not compare its area against the nine-facility
human reference or report it as the complete enriched-iron factory.

| Fragment | Facilities | Straight: area / cells | Left: area / cells | Right: area / cells |
| --- | --- | --- | --- | --- |
| Battery furnace → component | 2 | 21 / 1 | 28 / 2 | 28 / 2 |
| Iron furnace → grinder → thickener | 3 | 72 / 2 | 77 / 6 | 77 / 5 |
| Liquid Xiranite → polymer liquid | 2 | 55 / 1 | 77 / 3 | 77 / 3 |

All nine family results validate their complete internal chain. The first two use belts; the
third uses pipes from the real catalog. All retain their original pending external obligations.
The existing segment-only turn score is zero for these selected short routes. That does not
mean the material path is geometrically straight through the facility-port arms: folded
approaches can turn within an endpoint cell. Do not compare this metric with the reference
report's explicitly port-arm-inclusive visual turn count.

The iron trace shows three accepted states for every family: T, then T+G with one routed edge,
then T+G+F with both edges routed. The straight family selects centered source ports and gap 1.
The bent families also select some offset ports. Both gap values and all attempted poses remain
in the decision trace, including valid candidates not selected by the current-area baseline.

Work totals across all families, including one shared validated seed:

| Fragment | Charged poses | A* expanded states | Independent validations |
| --- | --- | --- | --- |
| Battery | 54 | 252 | 55 |
| Iron | 108 | 612 | 109 |
| Pipe | 24 | 144 | 25 |

The final replay runs finished construction in approximately 0.21, 0.39 and 0.11 seconds,
respectively, while parallel runs were active. These are operational observations, not a
controlled performance comparison. Input/catalog preparation and HTML rendering are outside
the reported construction interval. Proposal generation, routing and validation share the
same per-run limits across all families. Expiration is checked between operations and caps
the validator process; A* itself is bounded by expansion count and its finite-grid guard.

## Failure and regression evidence

With the iron proposal budget reduced to 1, the CLI returns `chain-stopped`, retains the
independently validated T+G prefix, writes no selected completed-chain witness, and records
unattempted families. Separate tests exhaust route-expansion and validation counts and reject
a validation result arriving after the shared deadline. A failed independent validator cannot
commit a candidate or cause growth from it. Failed smaller prefixes are never ranked against
completed families. Reusing an output directory rejects without changing the prior report.

The renamed-ID regression initially exposed a JavaScript dictionary bug for an upstream
facility named `__proto__`: ordinary assignment changed the utilization object's prototype
instead of adding its own key, and the independent validator rejected the missing utilization.
The local regression confirmed the issue. Computed-property object creation fixes it; renamed
IDs, reversed input arrays and repeated runs now preserve generated geometry and work counts.
No recipe or facility identity selects a hardcoded pose.

Malformed ledgers, fixed geometry fields, invalid rates, unknown catalog items, over-capacity
internal lanes, and unsupported topology reject explicitly. The real citron reference converted
to geometry-free graph data is deliberately rejected as `chain-unsupported-topology`: its cycle
and branch are milestone 2, not proof that the factory is infeasible. The pipe-order check also
rejects a sink-first chain that would route a pipe after a belt. Positive mixed-transport full
factory coverage is not claimed by this milestone's all-belt/all-pipe controls.

## Reproduction and artifacts

```sh
cargo build --release -p aic-cli --bin aic-witness-probe
node docs/benchmarks/chain-prototype/run.mjs \
  --request docs/benchmarks/chain-prototype/iron-chain.json \
  --recipes data/game/normalized/recipes.json \
  --facilities data/game/normalized/facilities.json \
  --items data/game/normalized/items.json \
  --transports data/game/normalized/transports.json \
  --components data/game/normalized/logistics-components.json \
  --validator target/release/aic-witness-probe \
  --output-directory target/chain-m1-iron-replay
node --test docs/benchmarks/chain-prototype/*.test.mjs
cargo check --workspace
cargo test --workspace --quiet
```

Use `battery-chain.json` or `pipe-chain.json` with a fresh output directory for the other
fragments. Final measured artifacts are under `target/chain-m1-{battery,iron,pipe}-release/`.
Each has `report.json`, `selected-witness.json`, and `straight.html`, `left.html`, `right.html`.
Every validation has its own immutable subdirectory with witness, independent report,
decision metadata, process result and log. Family histories point to accepted artifacts.
Reports hash runtime inputs, the validator and the implementation files used for the run.
Earlier `-initial`, `-verified` and `-final` directories are not the final provenance/UI replay.

All 12 prototype tests pass. The broader prototype, reconstruction and constructive Node suite
passes 304 tests. Rust workspace check/tests pass: 524 passed, two ignored. No Rust files or
dependency manifests changed. Browser controls pass six viewport/theme cases on each of the
three iron family pages, plus the battery-right and pipe-left pages (30 cases total). The iron
straight/left/right full views were personally inspected. The additional two page inspections
are recorded with their browser artifacts. Final browser directories are
`target/chain-m1-<fragment>-<family>-release-browser/`. The prototype wrapper hides the unused
second commodity legend for single-commodity fragments without editing the shared viewer.
A CLI regression executes that wrapper and checks legend visibility and scope after redraw.

## Policy audit and next milestone

This slice follows the accepted constructive planner's routed-transaction rule and the user's
request for a separate staged prototype. It uses runtime game data, no power, no global exact
search, no production fallback and no user-specified blueprint bounds. The 2–16-node input
guard and finite A* canvas are diagnostic resource limits, not game limits. No custom propagator
or internal architecture cutover was introduced. Unrelated dirty breadth and shared-view files
remain outside this commit.

The result establishes candidate generation and transactional routing, not that current-area
ranking captures the user's completed-envelope reasoning. On these tiny fragments, straight
is smaller; no universal dominance rule follows. Each family currently uses one repeated turn
pattern and one seed-orientation preference. Mixed bend sequences, future-body fit, rollback
and actual routed completion evaluation must be tested in later stages.

The user independently observed that the two displayed small fragments bend unnecessarily
and lose quality. The measurements agree: battery area increases from 21 to 28 and pipe area
from 55 to 77, with more transport cells. These are capability controls, not evidence that
bending is beneficial. Future proposals must allow selective bends and preserve straight
continuations; a bend earns selection only through its effect on routed completion with the
other chains and consumers present. Repeating a bend at every link is not the user's strategy.

Next: extend the contract to the citron seed cycle and its branch. Construct the cycle and
dependent growth group privately, route each introduced connection incrementally, and commit
only after the group's internal closure validates. Reuse exact seed split rates from milestone 0,
but no screenshot poses. If the cycle cannot close, preserve the failed port/route trace and
compare it to the user's short upper/lower return connections before increasing search bounds.
