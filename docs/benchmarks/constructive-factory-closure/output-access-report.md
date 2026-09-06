# Pending-output access and closed-pocket footprint checks

Status: growth defects corrected; density-cut construction now has one complete
candidate. The best complete factory remains **3,224 / 775 / 119** (area / physical
transport cells / turns); the new density-cut result is not promoted over it.

## Two distinct traps, verified visually

1. In the earlier rectangular-source 60%-fill-cut control, F11 was placed immediately
   west of F17. Its output connection cells (61,3), (61,4), (61,5) all lay inside
   F17's body. The input-only transaction nevertheless accepted F11. A regression now
   reproduces this adjacency from the actual runtime furnace/thickener catalogs.
2. With immediate output access checked, the same packing progresses from 52 to 55
   facilities but traps F14. All three F14 output ports enter one closed component:
   x=36..46,y=39..40 plus x=36..39,y=41, exactly 26 cells. F09 requires a 5x5 body.
   Area-only screening would pass 25+1 cells, but no such footprint fits in this
   at-most-three-row pocket. Native rendering and a complete flood confirm this.
   A diagnostic A* check of all 15 F14/F09 port pairs for the first failed placement
   also finds no path in the measured extent plus four cells; this is not presented
   as a global infeasibility proof or substituted for the closed-component evidence.

## Implementation

`pendingOutputAccess` derives exact output margins from graph requirements minus
already routed facility-output terminal rates. It aggregates compatible item fragments,
then rounds each distinct remaining item's rate up to catalog line capacity. Count
unused output connection cells after checking body and same-layer occupancy. Check
all retained owners, not just the added facility. No remaining margin means no reserved
ports; two free ports can satisfy a one-port demand even if another unused port is blocked.

`pendingOutputSpace` then probes free components from those connections. A completely
explored closed component must contain an allowed pending-consumer rectangle without
covering the chosen source connection or any existing body/belt/pipe cell. Different
transport layers are independent for flood connectivity but both block a new body.
Consumer rotations and footprint sizes come from runtime catalogs. External exports
cannot end in a fully closed component. Per-candidate component caching avoids probing
the same closed pocket separately for adjacent ports.

Floods reaching the measured perimeter or the 4,096-cell diagnostic work limit are
allowed as unknown, not rejected. Footprint fit is only a necessary condition; it does
not promise simultaneous packing of several consumers, compatible port orientation,
multi-input access or end-to-end routing. These are fixed-interior/unused-port checks,
not proofs excluding relocation, retained-wire replacement or a new structural crossing.

Both checks run after private input routing and before successful-candidate copying or
acceptance. Stable errors identify owner, transport, pending rates, port availability,
target dimensions and pocket evidence. The accepted base remains untouched. Over-routed
outputs produce `closure-output-margin-exceeded`; development tests caught and corrected
an initial attempt to represent a negative value with the nonnegative rational utility.

The research growth contract is now simpler to interpret: a successful private candidate
has routed inputs and does not knowingly strand its pending outputs in an unusable cell
or closed pocket. No Rust production API, game rule, score priority or validator changed.

## Controlled experiments

All matrices use the same source banks, 12 outer nesting placements and two corridor
families (24 completion attempts). The 60% control cuts later structured growth at
14 and 27 facilities, after the first shared-consumer join. It is still a restricted
prefix experiment, not arbitrary SCC-seeded partitioning or two-step lookahead.

| Growth / source arrangement | Original | Immediate access only | Access plus pocket fit |
| --- | --- | --- | --- |
| Full growth, occupied source packing | 1/24 complete | 1/24 complete | 1/24 complete |
| 60% cut, occupied source packing | 0/24, max 53 facilities | 0/24, max 56 | 0/24, max 56 |
| 60% cut, rectangular source stack | 0/24, max 52 facilities | 0/24, max 56 | **1/24 complete** |

The full-growth complete seed retains exactly the previous witness SHA-256
`683cd488b42cc9cbdf4c4d7eed17b4422ff3fb32dbcd10c547278d9ea20390cd`, score
5,040 / 1,375 / 184. Thus the successful construction used for the 3,224 incumbent
remains available. In the specific packing-2 replay, the second check stops earlier
at 54 rather than accepting the trapped F14 as apparent progress; report this honestly.

The newly complete threshold case is rectangular-source packing 1, simple corridors:
81x68 / **5,508 / 1,262 / 147**. Three rounds of existing connected-group relocation
and terminal-flexible A* rerouting reach 72x56 / **4,032 / 879 / 103** in 1,032 trials.
This is worse than the 3,224-area complete incumbent and is not promoted. It contains
1,936 occupied XY cells and 2,096 empty cells. Actual native inspection still shows
large upper-left voids and long process/pipe corridors.

Single observed elapsed times, under concurrent experiment load: current full-growth
matrix 84.438s, shape-cut matrix 171.410s, rectangular-cut matrix 182.460s; local
improvement 37.658s. These are whole matrix costs, not a claim that one factory costs
three minutes or that the new checks caused the elapsed-time difference. Solver-stage
construction and per-validation times remain in the JSON reports.

## Verification

- 114 related Node tests pass. The final 14 closure tests also pass after adding
  explicit overproduction and successful-pocket assertions.
- New tests cover the actual F11 adjacency, exact residual rate arithmetic, compatible
  fragments versus distinct items, independent transport layers, components, negative
  coordinates, the 26-cell false area positive, source-cell exclusion, opposite-layer
  body obstacles, component caching, capped/open uncertainty and closed external sinks.
- `cargo check --workspace` passes; `cargo test --workspace`: 524 pass, zero failures,
  two existing ignores. No Rust edits.
- Eight complete-factory audit controls pass separately for the retained 3,224 factory
  and new 4,032 density-cut factory, both against the original graph. Both retain all
  59 facilities, 131 recipe margins and seven true external obligations.
- Native access-only, pocket-rejection, highlighted closed-pocket, raw complete and
  refined complete renders personally inspected. Browser runs verify six theme/viewport
  combinations plus zoom, pan and Fit. The highlighted historical pocket intentionally
  displays the rejected trap, not the newly accepted layout.
- Current-source hashes are recorded in the final matrices and nested closure reports;
  the immediate-access-only observations remain development controls at their recorded
  source hashes. The initial failed arithmetic draft is not included as research evidence.

## Evidence and next gate

- `output-space-full-control.json`: successful full-growth path retained.
- `output-space-shape-control.json`: current 60%-cut occupied-source matrix.
- `output-space-shelf-control.json`: current 60%-cut rectangular-source matrix.
- `output-access-shelf-control.json`: immediate-access-only development control.
- `output-space-refined.json`: new complete threshold factory's local improvement.
- `output-space-retained-audit.json`, `output-space-refined-audit.json`: independent audits.

Rendered experiment: `target/shared-results/bound-interconnect-closure-density-refined.html`.
Highlighted trap: `target/shared-results/bound-interconnect-closure-output-pocket-pocket.png`.
The best result stays `bound-interconnect-closure-shape-refined.html` (3,224 area).

Next: replace the forced first shared-consumer join/two-large-cluster assumption with
genuine SCC-seeded stop/grow alternatives. Compare one/two-facility extension, productive
fill, exact pending interfaces and assembled whole-factory area. A module boundary is an
ownership/interface decision, not a reason to silently omit upstream obligations or
duplicate a shared supplier. Preserve the current complete incumbent throughout.
Do not decide the user's proposal from this restricted threshold control alone. Shared
five-member-cycle quality, final compactness, recipe breadth and later product stages
remain open. No global optimality, no global infeasibility and no goal-completion claim.
