# R0: directional frontier evidence and feedback review

## Outcome

Specify direction-guided active-interface growth as the next controlled experiment. Do not adopt
it yet or claim a new complete layout. The baseline constructor, independent validators, official
score, output format and shared HTML are unchanged.

The [research contract](../../designs/2026-09-05.01-directional-frontier-research.md) incorporates the
user's proposed input/output growth direction and the supplied researcher feedback. The memo's
references are not independently reviewed here; observations below come from local artifacts.

## Measured evidence

These are offline measurements of the existing Q0 witnesses, not new construction timings.
`evidence.json` contains source/report hashes, original-wiring projection hash, all boundary owners,
one-cell strip objects and incident-frontier counts after each accepted step.

| Witness | Covered facilities | Last internal-stage bounds | Final reported bounds | Complete |
| --- | ---: | --- | --- | --- |
| Coarse baseline | 59 | 165 x 132 | 165 x 140 | Yes |
| Guarded nearby N=4 | 27 | 93 x 21 | 93 x 21 | No |

The baseline internal-complete area is 21,780; external completion adds 1,320 (6.06% of internal
area, 5.71% of final area). Thus removing all external-stage area overhead would leave most of the
existing area. This does not prove any individual external route is optimal or removable.

Baseline boundary owners:

- Left: a facility and transport geometry.
- Right: one facility, with no transport geometry at that extreme.
- Top: transport geometry and all seven external terminals.
- Bottom: transport geometry.

Therefore moving routes alone cannot reduce baseline width with facilities fixed. A left-strip
repair must consider both facilities and routes; a right-strip repair would still have to repair
connections to the moved facility even though they do not co-own the current extreme.

All four extremes of the N=4 partial state's rectangle are held by transport geometry, not
facilities. This supports inspecting blocking routes as well as facility packing. It does not prove
that all those extreme cells obstruct growth, nor that removing them would produce a valid repair.

Original-ledger audit of that partial state:

- 20 facilities have no uncovered incident requirement.
- Five facilities have incoming requirements only; two have both incoming and outgoing requirements.
- Nine incident requirements remain: three internal recipe flows, four external inputs, one surplus
  output and one target output. All nine are present in the node's boundary projection.
- Every projected requirement has one attachment, all of kind `new-port`; their physical port edges
  include north, east, south and west. No existing-network attachment is projected in this snapshot.
- The target itself retains a south-facing input and north-facing output in its current rotation.
  Preserving the outward expansion of just its input is insufficient.

These 20 incident-closed facilities are not automatically safe to move, freeze or bury: shared
network segments can still need future expansion. Projected options are the constructor's current
options, not a complete enumeration of game-legal placements, ports or attachments.

The stored Q0 rejection connects `wiring-edge:8dee9d24d17b98cf3cc8798a9c28a04499b0a77a3d7dea14045c6e199831aee7`
and blocks pending final output
`wiring-edge:7565bb835dbe056399c0de47fcf3bd21108488893615068a6cba7f69078572de`.
The rejected trial is distinct from the accepted final partial node. Future-direction guidance
must protect outputs and external demands throughout growth, not just the next upstream input.

## Offline diagnostic contract

Input: production report schema 2 with a final node and retained steps, successful original wiring
projection generated from external recipe/source-plan files, and runtime component catalog.
The CLI takes explicit paths and creates a new output file exclusively. No dependency additions.

Output schema 1: provenance hashes, completion flags, internal/final geometric boundary owners,
one-cell outer-strip object references, external-stage area delta when internally complete, and
per-step original-ledger incident state counts. Detailed output is JSON; stdout is one completion line.

Geometry includes rotated facility extents already recorded by the planner, network cells/segment
endpoints, catalog-checked single-cell components, and external terminal positions. Co-owners are
all retained. Cell and segment references overlap and must not be summed as physical tile counts.
Segment indices and cell coordinates are snapshot-local, not persistent IDs for future repair.

Fail on missing/unsupported component footprint, invalid geometry or strip width, absent final
node, wrong report schema, mismatched used bounds, wiring projection failure, malformed input or an
existing output file. Larger component footprints require an explicit anchor contract before this
audit can support them. This is an offline evidence reader, not a replacement validity/capacity oracle.

## Reproduce

From the repository root, with the retained Q0 raw reports (or regenerated equivalents):

```sh
node --test docs/benchmarks/constructive-directional-frontier/audit.test.mjs
node docs/benchmarks/constructive-directional-frontier/audit.mjs \
  target/release/aic-cli \
  data/game/normalized/recipes.json \
  data/examples/source-plan.game-heavy-xiranite-forge.request.json \
  data/game/normalized/logistics-components.json \
  /tmp/aic-directional-audit-new.json \
  /tmp/aic-placement.Zlt9PS/guarded/heavy-xiranite-0-baseline.json \
  /tmp/aic-placement.Zlt9PS/guarded/heavy-xiranite-0-nearby-4.json
```

Use a fresh output path. Raw `/tmp` artifacts are not guaranteed permanent; regenerate baseline and
N=4 with the [Q0 harness](../constructive-nearby-placement/run.mjs) if needed. Timing-bearing report
hashes may change across regeneration; the deterministic final-node hashes should match.
Recipe, source-plan and component hashes used here match the original guarded Q0 benchmark inputs.

## Verification and remaining risk

- Five Node tests pass: translated/inclusive bounds and co-owners; segment ownership; unsupported
  component/bounds failures; original-ledger external and both-placed demands; shared attachment
  classification and incident-only closure.
- Manual audit of both stored full/partial reports succeeds, with short stdout and file-backed JSON.
- `cargo check --workspace` passes. `cargo test --workspace` passes all 495 tests with two existing
  research ignores. No Rust production source changes.
- No reachability, joint port feasibility, SCC order or global direction is inferred from a geometric
  extreme. R1a must still test whether directional ordering actually changes completion favorably.
- The follow-up old-pose/best-of control and equal-work counters requested in the feedback remain
  unmeasured. Do not claim causal separation of every Q0 factor or an equal-budget quality gain.
