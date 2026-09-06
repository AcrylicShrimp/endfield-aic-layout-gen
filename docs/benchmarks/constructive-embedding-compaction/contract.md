# Fixed-order embedding compaction

Status: experimental complete-witness improvement, not a constructor or production cutover.

Input: a concrete independently validated complete factory, original graph and runtime catalogs.
One pass moves coordinates on one axis, keeping the perpendicular coordinates unchanged. Preserve
facility dimensions/rotations/utilizations, exact terminal identities/rates, component identities,
directions and the directed transport topology after suppressing constant-rate straight interiors.
Keep all original external boundary sides. No additional crossing or shared cell may be introduced.

Build critical geometric points from facility cells, components, terminals, bends, junctions,
rate transitions and every cell of a perpendicular segment. A point critical for either transport
layer is critical for both. Suppress only noncritical constant-rate directed straight interiors.
Coincident points on independent layers remain tied; this is conservative, not a game restriction.

Rigid facility offsets, perpendicular segment alignment and facility-terminal attachment are
equalities. Contract their connected components with integer offsets. For consecutive critical
points in each perpendicular row, preserve their strict old axis order with one-cell separation.
Horizontal (or vertical after axis exchange) chains then span consecutive retained events in
their own network. Row order prevents their interiors from entering a previously disjoint object.
Virtual low/high boundaries enclose every point and retain external terminals on their old sides.

The resulting difference constraints have form X[v] >= X[u] + weight. Their original coordinates
are a feasible certificate. Starting from nonnegative coordinate lower bounds, longest-path
relaxation gives the componentwise least feasible solution. This minimizes the moving-axis extent
for this fixed perpendicular geometry, row order, coincidences and suppressed-chain topology.
Then fix that minimum extent and minimize physical parallel-chain length using the bounded integer
minimum-closure model in `length-contract.md`. The secondary solution need not be componentwise
least; it preserves the minimum extent. Neither phase minimizes arbitrary layout area.
Re-expansion restores unit segments with exact rates and checks predicted versus actual tile change.
Original coordinate feasibility is checked before solving. Explicit point/relaxation/materialization
budgets report a diagnostic limit, never global infeasibility; input geometry is never mutated.

Output: detached candidate witness, recomputed catalog ports, invalidated acceptance flags,
pre-compaction planning metadata, coordinate/constraint certificate and separated timings.
Diagnostics carry stable code, severity, stage, message and relevant entity. The CLI validates
the incumbent first, then each candidate with the independent release validator and exact original
graph coverage. Only a complete lexicographically better (area, tiles, turns) result replaces it.
Alternate x/y passes under an explicit iteration budget, stopping after an unimproved pair.
No production caller canvas, global exact solver, implicit recipe supply or fallback is introduced.

Verification: synthetic rigid bodies, terminals, reverse routes, touching obstacles, mixed layers,
crossings, changed rates and external sides; brute-force tiny difference systems; nonmutation,
detached outputs and idempotent axis passes; actual full witness with independent validation and
negative controls. Inspect actual rendered outcomes before claiming improvement.
