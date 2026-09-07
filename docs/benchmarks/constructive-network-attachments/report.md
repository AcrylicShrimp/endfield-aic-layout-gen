# Network-Aware Constructive Attachments

## Scope and contract cutover

This slice implements `../../designs/2026-09-05.00-network-aware-constructive-frontier.md`.
Composition can connect a new facility port or extend a compatible existing commodity network
through a placed catalog splitter/converger. Every shared segment, terminal, and component must
have sufficient residual capacity. An independent grid-witness validator checks facilities,
ports, directed adjacency, flow conservation, capacity, component arms/rotations, layer occupancy,
and supply-to-demand reachability before a candidate can replace the current valid node.

The obsolete point-to-point composition validator and ordered-cell-path scoring are removed.
Network cells are a set; directed segments carry flow. Compatible routes are normalized into one
network per item and transport, including disconnected parallel lanes. Consequently, network
counts before and after this cutover are **not** directly comparable physical-route counts.

There is no exact model construction, exact-search fallback, caller W/H constraint, rollback,
trunk resynthesis, or new global optimization phase in this slice. Process-module internal growth
still uses capacity-bounded point-to-point lanes. Automatic discovery still composes rigid modules.
Boundary residual credit is an optimistic screen, not a reservation guaranteeing all future
requirements can be embedded simultaneously. Final committed geometry is independently validated.

Boundary attachment enumeration shares a node occupancy index across all requirements. Saturated
terminals are rejected before anchor path enumeration: every possible attachment would increase
the same terminal flow, so this eliminates no capacity-legal candidate. Preliminary builds without
these checks were stopped manually and are not reported as completed planner failures or used as
the final performance measurement.

### Breaking inputs and reports

`construct-process-module`, `compose-process-module`, `assemble-process-modules`, and
`auto-assemble-process-modules` now require `--logistics-component-catalog`. There is no empty
catalog default. Boundary `port_options` is replaced by tagged `attachments` with `kind` equal to
`new-port` or `existing-network`. Existing-network evidence includes the facility port, terminal,
network, anchor, branch cell, component placement, shared segment indexes, and residual capacity.

Report schema versions are process module 2, composition 5, recursive assembly 2, automatic
assembly 5, and port-demand analysis 2. Old reports must be regenerated; there is no legacy DTO
adapter. Port-demand `required_ports` now means additional unused ports after reusable-terminal
capacity credit, with `rate_requiring_new_ports` and `reusable_terminal_capacity` exposing the
calculation. Geometry failure reasons replace the obsolete ordered-path validation counters.

## Controlled fixtures

The external synthetic facility catalog is `../../../data/examples/constructive-network.facilities.json`.
Each machine has one input and one output. The actual normalized transport and logistics-component
catalogs supply line capacity, component definitions, arm directions, and allowed rotations.

| Case | Demand | Result |
| --- | --- | --- |
| Fan-out | One producer, two consumers, each `1/5 item/s` | One producer port, one real splitter, three terminals, `3x3` used bounds |
| Merge | Two producers, each `1/5 item/s`, one consumer | One consumer port, one real converger, three terminals, `3x3` used bounds |
| Overload | Two demands, each `3/10 item/s`, one producer port | Rejected; `3/5` cannot fit the catalog `1/2` line |

The shared fan-out and merge segment carries `2/5 item/s`; its residual is `1/10 item/s`.
Each unshared branch carries `1/5`, leaving `3/10`. All four rotations and a translation pass
independent validation. Removing a component, changing its rotation, moving a terminal, or
overloading a segment makes the witness fail validation. Composition leaves its input nodes
unchanged. Existing one-edge and parallel-lane tests remain in the regression suite.

`fan-out-point-to-point.*` and `merge-point-to-point.*` show the first two-facility transaction;
`*-result.*` show the final shared network. The overload is rejected proactively during that
first transaction's remaining-boundary capacity audit, so both overload artifact names contain
the same failed report, not a purported valid point-to-point factory. It rejects 389 complete
bundles: 12 have no remaining attachment, and 377 fail grouped remaining capacity.

HTML route-cell inspection exposes segment load, line capacity, residual, and component rotation.
Terminal inspection exposes aggregate port flow. Regenerate the controlled JSON/HTML with:

```bash
cargo test -p aic-data write_network_attachment_artifacts -- --ignored --nocapture
```

## Heavy Xiranite measurement

The release run finishes with structured `constructive-automatic-assembly-exhausted` (CLI exit 1),
not a timeout. Eleven attachments are accepted; the twelfth frontier has eight generated
candidates, two member-overlap rejections, and six failed compositions. No module-construction
failure remains at this frontier. The previous module-construction failure now reaches the
member-overlap check, so the new attachment contract does improve that candidate's boundary
representation, but it does not solve module ownership.

| Metric | Previous checkpoint | Network-aware checkpoint |
| --- | ---: | ---: |
| Accepted attachments | 11 | 11 |
| Facilities / used bounds | 28 / `68x16` | 28 / `68x16` |
| Physical transport tiles | 162 | 162 |
| Commodity networks / explicit components | Not normalized | 8 / 0 |
| Rigid placements at exhausted frontier | 41,502 | 41,502 |
| A* calls | 505,325 | 847,767 |
| A* failures | 216,173 | 377,949 |
| Completed bundles rejected by remaining boundary | 186,951 | 367,617 |
| Successful A* calls in incomplete bundles | 102,201 | 102,201 |
| Complete bundles rejected by grouped capacity / geometry | 0 / 0 | 0 / 0 |
| Valid final-frontier compositions | 0 | 0 |

The final run takes 550.73 s wall, 1500.01 s user, and 13.01 s system time. Accepted discovery steps
account for 59.730 s. A separate final-code eleven-step probe takes 72.06 s and reproduces the
identical final node, including boundaries, geometry, networks, and capacities. That shorter probe
overlaps approximately 72 s of the longer run, so these are not isolated speedup/regression ratios.
The historical 54.46 s is also a host-local observation. The durable regression evidence is the
larger candidate count and the unchanged accepted frontier. Preliminary stopped runs are excluded.

The generalized lane transaction no longer uses the old strictly increasing source-port index;
it enumerates attachment orderings as well as endpoint choices. This helps explain increased work
even when no shared branch is selected. Node cloning and full boundary refresh add per-candidate
cost. Occupancy caching and saturated-terminal rejection avoid some redundant work, but this slice
does **not** claim a performance improvement.

| Candidate requirement prefix | A* calls | No route | Complete bundles | Outcome |
| --- | ---: | ---: | ---: | --- |
| `2c69b956` | 24,624 | 24,624 | 0 | No first route |
| `3983a3f0` | 171,782 | 44,437 | 97,210 | Remaining boundary lost |
| `6a3c9cb8` | 17,520 | 17,520 | 0 | No first route |
| `6b0699f3` | 327,855 | 122,974 | 168,977 | Remaining boundary lost |
| `91a73c19` | 263,796 | 132,231 | 101,430 | Remaining boundary lost |
| `b29b99f9` | 42,190 | 36,163 | 0 | Incomplete two-lane bundle |

All 367,617 complete bundles lose at least one remaining boundary attachment. Per-requirement
rejection counts overlap: one bundle may block several requirements. Their sum is therefore not
another count of unique bundles. The six exhausted compositions make zero existing-network
attempts. Saturated-terminal checks and terminal-only short internal routes leave no eligible
non-terminal attachment anchors. Zero branch attempts does not prove that legal shared-network
geometry could not be constructed differently.

## Next bottleneck: compact immutable modules with conflicting exits

The user challenged whether rollback was necessary. The answer is **not established**. Three
composition candidates physically connect their current input; they fail the future boundary
condition afterward. Calling this an inability to wire any new module is incorrect.

`moss-module.json` and `moss-module.html` reproduce the fresh source of candidate `6b0699f3`.
Its local `13x5` packing is:

- planter at `(0,0)`, size `5x5`, rotation 90;
- grinder at `(5,1)`, size `3x3`, rotation 0;
- planter at `(8,0)`, size `5x5`, rotation 270.

The grinder must export `1 item/s` of moss powder, requiring two `1/2` belt lanes. Its exposed
output cells are `(5,0)`, `(6,0)`, and `(7,0)`. The two planters each retain a separate `1/4 item/s`
moss output for seed production. Their only currently exposed output cells are `(5,0)` and `(7,0)`.
These are different commodities from grinder output and cannot occupy the same belt cell.

Thus this **fixed attachment domain** needs four distinct output cells but has only three. Every
choice of two grinder outputs consumes at least one remaining planter output. Moving or rotating
the whole module cannot remove that internal conflict. The source module's top lost requirements
are `a2a0229a` (113,700 occurrences) and `1003da2b` (113,087), exactly those planter outputs.

There is spare capacity in the internal moss routes (`1/4` on a `1/2` line), but their one-cell
and two-cell paths contain only facility terminal cells. The current attachment generator excludes
terminal cells from anchors, so it cannot expose that residual as a branch. This is an
implementation restriction, not a new proof about game legality. The semantic scope of explicit
branch components at terminal-adjacent cells needs a separate review before changing that rule.

The immediate failure can therefore originate in a **new, not-yet-accepted module**, rather than
in a need to undo the existing 28 facilities. Regenerating its internal placement/routing with room
for both outputs is a more direct experiment than increasing whole-factory rollback.

Reproduce the source module without composing it into the existing factory:

```bash
aic_moss_root=$(jq -r '.root_instance' artifacts/benchmarks/constructive-network-attachments/moss-module.json)
target/release/aic-cli layouts construct-process-module \
  --recipes data/game/normalized/recipes.json \
  --source-plan data/examples/source-plan.game-heavy-xiranite-forge.request.json \
  --facility-catalog data/game/normalized/facilities.json \
  --item-catalog data/game/normalized/items.json \
  --transport-catalog data/game/normalized/transports.json \
  --logistics-component-catalog data/game/normalized/logistics-components.json \
  --root-instance "$aic_moss_root" --internal-item item-plant-moss-2 \
  --visualization-output artifacts/benchmarks/constructive-network-attachments/moss-module.html
```

## Fixed-layout accessibility and bridge probe

`boundary-access.mjs` independently flood-fills free cells in the measured layout, with a one-cell
exterior ring around its used bounds. This is an analysis extent, not a planner canvas ceiling.
It treats facilities and same-layer routes as obstacles and reports every unresolved input port.
Nine of twelve frontier requirements have at least one exterior-reachable port; three have none
under the current bridge-free router. Port availability alone does not establish simultaneous
lane routability, module packing, or completion of the factory.

A second diagnostic traversal admits straight perpendicular crossings at eligible existing line
cells, using the catalog bridge capacity and excluding terminals, turns, and branch components.
Under the repository's current bridge model, all three otherwise enclosed boundaries gain an
exterior access path without relocating facilities or removing existing segments:

| Requirement prefix | Example input cell | Added bridge cells |
| --- | --- | --- |
| `6a3c9cb8` | `(6,13)` | `(7,14)`, `(7,15)` |
| `2c69b956` | `(54,2)` | `(54,0)` |
| `95483e21` | `(61,1)` | `(61,0)` |

This is a **single-port accessibility probe**, not a placed complete module, a reserved multi-lane
solution, or an independently validated production bridge witness. It checks existing-axis
capacity and geometry; it does not allocate all future traffic. The mathematical model explicitly
retains game-evidence ambiguities about bridge adjacency and elevation. In particular the adjacent
two-bridge example is conditional on the repository model, not newly confirmed game behavior.
The constructive generator and validator currently do not support physical bridges.

Reproduce accessibility evidence with:

```bash
node docs/benchmarks/constructive-network-attachments/boundary-access.mjs \
  artifacts/benchmarks/constructive-network-attachments/heavy-xiranite.json \
  data/game/normalized/logistics-components.json
```

## Research decision

Do not conclude that heuristic full-factory construction is intrinsically this expensive or that
rollback is mandatory. The implementation currently packs modules tightly, freezes their internals,
then enumerates rigid compositions and prefers the smallest partial result. That differs from the
base design's reliable, naturally sized complete constructor followed by improvement.

The next slice should restore construction-first behavior: attach one frontier facility or atomic
cycle, retain room and concrete capacity for its other required connections, and route immediately.
Reconstruct the uncommitted local module when its interfaces conflict; reuse already-present
facilities rather than rejecting an otherwise useful producer because a predefined module overlaps.
Joint boundary-cell demand and region reachability should diagnose contradictions before hundreds
of thousands of A* calls. Test bridge-aware routing under an explicit validated contract. Only then
use bounded rollback when the accepted local history, rather than the proposed new module, is the
confirmed obstruction. Area, tile count, and turn optimization retain their priority in the later
improvement phase; they are not the initial completion gate.

Preserve the shared-network DTO, capacity checks, independent validation, and diagnostics from this
slice. Do not add a larger exact solver or discard validation to obtain a demonstration. This work
stops at the evidence-backed architecture decision; it does not implement the next constructor.

## Reproduction

```bash
cargo build --release -p aic-cli
/usr/bin/time -p target/release/aic-cli layouts auto-assemble-process-modules \
  --recipes data/game/normalized/recipes.json \
  --source-plan data/examples/source-plan.game-heavy-xiranite-forge.request.json \
  --facility-catalog data/game/normalized/facilities.json \
  --item-catalog data/game/normalized/items.json \
  --transport-catalog data/game/normalized/transports.json \
  --logistics-component-catalog data/game/normalized/logistics-components.json \
  --target-instance 'facility-instance:recipe-occurrence:/target:0' \
  --max-steps 12 \
  --visualization-output artifacts/benchmarks/constructive-network-attachments/heavy-xiranite.html \
  --report-output artifacts/benchmarks/constructive-network-attachments/heavy-xiranite.json \
  --localization-catalog data/game/normalized/localization.ko-KR.json
```

Elapsed times are host-local observations, not performance assertions. A structured constructive
failure preserves the last valid partial layout and is not global infeasibility evidence.

## Verification and reconstruction inputs

Implementation: `443f643` (`Support capacity-safe constructive commodity network attachments`).
Host: Darwin arm64, Rust stable `1.98.0 (88d9e12ae 2026-08-18)`. Release build time is excluded
from the CLI wall time. Normalized runtime inputs are unchanged by this slice.

| Input | SHA-256 |
| --- | --- |
| `recipes.json` | `0fa9f71b6ab2c66ef774ad54f27ddac8682c4ccd6134730d69ca7506836f1ad2` |
| `facilities.json` | `db398048ac401024f7bae80c05ae59d4a9fb9a037f6836f33d26c5d08f0bcf96` |
| `items.json` | `b49b70d60bee6dc421817e0f6a793532866dfe8618c2e9111c17d032e3fc9164` |
| `transports.json` | `c3e7c7b324ad175870b0d61830d8df9296dc6a67791fd48d7c4f474a86cee8d1` |
| `logistics-components.json` | `3d7e3e59f1fef8d534c5aab5ba44d60b7ea759e2307c7ad60a7d2cc4def80608` |
| `source-plan.game-heavy-xiranite-forge.request.json` | `5302d96a5177ed2f3fde55fe13539187f7601b6667b0f0588980dca01ccd320c` |

Verification commands: `cargo fmt --all`, `cargo check --workspace`, `cargo test --workspace`,
the ignored artifact-generation test above, and the release CLI reproduction. The workspace
suite has 451 passing tests and one intentionally ignored artifact writer, which also passes
when explicitly selected. Headless Chromium verifies route-cell inspection metadata and renders
the controlled fan-out/merge and accepted growth pages for visual inspection.

No custom propagator changes are included; the independent witness validator is not a pruning
propagator and has no exact-model dependency.
