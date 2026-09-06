# Rigid facility-block rotation with boundary release

## Scope

Rotate a connected two-to-four-facility body arrangement as one rigid rectangle,
then translate it into a currently empty rectangle or a nearby origin. This changes
relative facility origins as well as their individual rotations. It is not equivalent
to rotating one detached member in place. Original network membership, terminal and
port IDs, recipe rates and external boundary positions remain unchanged.

Input: a complete validated witness, runtime catalogs, normalized original graph,
candidate limit 1..512 and combined-pose budget 1..65536 (default 16,384). Output:
schema-4 `moves` and `facilityRotations` requests plus group, quarter turn, origin,
optional inward-boundary closure, released bridges and screening evidence. The
request still contains at most four translation primitives and sixteen facilities.
An unsupported catalog angle, overlap, ownership conflict, body/terminal collision,
extent violation or exhausted release budget rejects that proposal, not the factory.

## Pose construction

The pure rigid transform takes the old body-envelope origin as its local origin,
rotates every body rectangle by one, two or three clockwise quarter turns, and places
the rotated envelope at the proposed integer origin. Non-square body dimensions must
rotate consistently with the catalog. Add the same quarter-turn delta to every member's
absolute angle. This preserves relative body and port geometry. Merge only identical
translation vectors into a primitive; reject more than four resulting primitives.

Use graph-derived pairs/cycles/serial groups and contiguous aligned body runs of size
two to four whose induced original dependency graph is connected. An aligned run is
a proposal source, not a new semantic grouping. Enumerate fitting integer origins in
up to eight measured empty rectangles, interleaved with the retained origin and
cardinal offsets of one to four cells. Sample groups/angles fairly under one combined
pose budget. Empty space is a placement hint, never permission to omit routing.

Additionally pair a block transform with existing inward boundary contact closures
of one to four cells when that original closure contains a transformed member. Remove
the block members from the closure's uniform translation and keep the remaining
members in one additional primitive. This bounded compound proposal can fill a void
and retract the boundary in the same transaction. Its rigid closure is only a seed;
validate the actual new combined poses against the original parent. Do not route or
validate a pre-rotated, incomplete intermediate witness.

## Validation and selection

Use the existing catalog projection and simultaneous movement gate on the unchanged
parent. Release the union of incident bridges and bridges covered by the new bodies,
at most eight. All affected routes are rebuilt by the existing engine; no rotated
route is silently copied. Keep retained external terminals fixed. Apply the existing
vacant-port repair, independent-layer compactor and whole-factory gate afterward.
Only an independently validated improvement to area/cells/turns can replace the parent.

Rank geometry survivors by fair per-group depth, full body-envelope area and endpoint
distance delta. Reports distinguish sampled poses, geometry survivors, shortlisted
positions, completed candidates, validated candidates and selected improvements.
Termination is bounded-neighborhood evidence, never local/global infeasibility or an
optimality certificate. The runner family `--move-family block-rotate` requires
move/joint mode and explicit bridge release. Production APIs are not changed.

Test rigid point/rectangle transforms and non-square catalog port coherence; connected
group derivation; four-primitive limits; combined boundary/block geometry; bounded fair
deterministic proposals and immutable input. Compare against the same-parent translation
and singleton-rotation controls. Independently validate selected factories and inspect
native full and focused views, including unsuccessful proposals when none is selected.

For a separate bounded component-pose replay, `componentPoseCandidates` accepts the
same optional facility rotation list as vacant/cooperative port domains. Its body and
terminal screening uses the proposed catalog poses, not translation-only approximations.
This helper does not move a junction automatically or change the main runner's portfolio.

A separately recorded lookahead control may start from one independently validated
but unimproved block audit and run at most three rounds of the existing push family.
That audit is a private search seed, not the selected factory. Preserve the global
incumbent while exploring it; report both the branch-local and global-root scores.
Only a complete validated result better than the global root may be promoted. This
tests whether filling a void exposes a later boundary contraction that a one-step
monotone acceptance gate misses. It does not add a production fallback or change
per-attempt movement/routing budgets.
