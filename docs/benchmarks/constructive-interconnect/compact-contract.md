# Direct monotone permutation channel (geometry schema 2)

Replace adjacent-exchange stages with a direct channel for the entire permutation.
Preserve exact lane IDs/items/rates and three-cell input/output boundary pitch.
Same-item lanes must preserve their relative order. No allocation changes, extra
capacity, implicit merge or unmodeled crossing are permitted.

For source index i and target index j, use row 3i and intermediate exit row
3j - sign(j-i), then a one-cell jog toward the final row 3j. An unchanged lane
stays straight. Every path is coordinate-monotone and Manhattan-shortest for
its fixed endpoints. Vertical tracks may share columns only when their closed
row intervals are disjoint. Endpoints face east at both boundaries.

For non-inverted moving pairs i<k and j_i<j_k, overlapping downward intervals
require track(k)<track(i); overlapping upward intervals require the opposite.
These are separate acyclic orders: downward edges descend source index and
upward edges ascend it. Non-inverted opposite-direction intervals are disjoint.
This avoids the pair's two otherwise redundant crossings. Inverted pairs cross
once in either track order. Straight lanes cannot cross a non-inverted path.
Thus only inversions cross, once each, and stable same-item lanes never cross.
The cell-walk checker and exhaustive tests verify this independently.

Choose a runtime-supported component/rotation for each actual crossing's arms.
Reserve its catalog-backed one-cell bridge footprint, not an empty 6x6 exchange
box. Unrelated paths cannot occupy that footprint. No turns at bridges, channel
mixing, parallel overlap or cell collisions are allowed. Emit structured failure
for unsupported arms, inconsistent geometry or explicit crossing/cell budgets.

Schema 2 removes `schedule` and reports `construction` with track order, shared
columns, precedence, inversion count and materialized path-cell count. The budget
diagnostic is now `interconnect-crossing-budget`, replacing `interconnect-swap-budget`.
This is a hard cutover in the shared research generator; old schema-1 witnesses
remain historical artifacts, not an alternate runtime implementation.

For two exchanged lanes the channel is 5x4 with 15 occupied cells, compared with
the former 6x6 / 21-cell template. For three reversed lanes the middle lane can
stay straight: 5x7 instead of three old exchange stages. These are concrete
constructions, not claims of global minimum area. Track coloring remains greedy.
Shortest paths apply inside this channel, not to surrounding approach/tree wires.

Verify every distinct-item permutation through six lanes, stable binary commodity
orders, both transport catalogs and adversarial geometry/catalog mutations.
Then independently validate recipe-bound local interfaces and complete Heavy
Xiranite reconstruction. Promote only a whole-factory area/tiles/turn improvement,
otherwise retain the complete incumbent. Inspect actual native renders. No Rust
production cutover, global infeasibility claim or completed broader goal.
