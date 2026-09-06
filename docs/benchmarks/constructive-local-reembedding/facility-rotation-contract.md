# Catalog-coherent local facility rotation

## Local pose schema 4

The local request retains `moves` (up to four translation primitives and sixteen
distinct facilities), and adds `facilityRotations: [{instance, rotation}]`. Each
rotation must name a facility already in `moves`, exactly once, with a quarter-turn
allowed by its runtime facility catalog. In-place rotation uses an explicit zero
translation primitive. An empty rotation list is translation-only, not a conversion
from an obsolete pose representation. New reports use move schema 4 and record the
rotation list; historical translation-only schema-3 requests need no inferred rotation.

Rotation is around the translated top-left footprint origin: width/height and every
port are projected from the catalog's canonical shape at the requested absolute
rotation. It is not rotation around a global grid point. Preserve terminal ID, port
ID, item, transport, direction, recipe and rate. Explicit port reassignment, if any,
is applied to this new projected shape afterward. Do not mutate or pre-rotate the
validated parent. The complete new pose is evaluated inside the existing transaction.

`planFacilityRotations(witness, catalogs, moves, rotations)` returns a map of changed
facilities containing the projected body, projected world ports, terminal positions
and quarter-turn delta. Invalid IDs, duplicate/over-budget entries, nonselected
facilities, unavailable catalog rotations or incompatible terminal ports return
`local-facility-rotation` with stage, severity, message and entity. Retained rotations
validate but do not count as a changed pose. No game data lives in this module.

## Movement and route invariants

Movement computes each terminal's actual displacement, including rotation. Shared
non-detachable owners must end at one common cell and have one common arm rotation
in this bounded neighborhood. Different quarter turns at a shared anchor are rejected
as `local-shared-terminal-rotation`, not a game-wide impossibility result. Recognized
detachable zero-length pairs may separate. Fixed external terminals remain fixed.

Nonterminal clusters retain the existing internal/downstream ownership policy, but
common displacement now means the actual adjacent terminal displacement. Cluster
movement is translation only unless an explicit component pose overrides it. A
facility's rotation never implicitly rotates its entire downstream network.

Replace internal `componentTurns` with `anchorTurns`: one map describes changed
directed endpoint arms, whether their owner is a component or a facility. Incident
chains of rotated anchors are rebuilt even when an endpoint's position is unchanged;
never carry an old internal route solely because its endpoint translations agree.
Retained endpoint arms rotate by the quarter-turn delta. Ordinary terminal-flexible
or explicitly reassigned ends retain their existing freedom. Body/terminal/arm
collisions, release limits, same-layer occupancy and complete-factory acceptance
remain authoritative.

Width and height of non-square bodies must participate in collision/bounds checks.
The no-change shortcut must include facility rotation changes. An unchanged position
or square body alone cannot hide changed ports. All final catalog port displays and
whole-graph validation must use the new rotation.

## Diagnostic and proposal integration

Priority/cooperative replays and vacant-port domains project the same rotation list;
they must not diagnose a rotated request using translation-only terminals. Rejected
native overlays may include explicitly projected facility poses and port stubs, marked
as proposals over the unchanged validated parent. Candidate witnesses still require
separate independent factory validation.

`--facility-rotations retained|catalog` applies only to the deflection proposal family.
The default is retained; catalog mode interleaves each detached member's legal rotations
within the existing combined-pose budget. Include same-translation/different-rotation
variants, project void fit using rotated dimensions, and expose the selected rotation
in the request. This is a bounded pose proposal family, not exact local packing.

The separate `--move-family rotate` control requires move/joint mode and bridge release.
It enumerates one facility's nonretained catalog rotations at its current origin and
cardinal translations of one or two cells. This separates the value of rotation from
the boundary group's inward movement. Sample facilities fairly within a 4,096-pose
budget, screen using the same movement gate, and select up to the existing 512-position
CLI limit. Release the union of incident bridges and bridges covered by the new body,
at most eight. Geometry screening is not full validation. Rank by fair per-facility
depth, body envelope and endpoint-distance delta. Every trial starts at the round's
validated parent; the normal improvement gate alone selects a new incumbent.

## Verification

Test non-square body/port projection against independent quarter-turn coordinates;
preserve IDs/rates and external endpoints; reject invalid/duplicate/unsupported
rotations; test rotated-body collisions and current-extent rejection; exercise shared
owners, detachable pairs, in-place arm changes and no-change handling. Compare legacy
translation requests and run full regression tests. Independently validate any new
complete factory and visually inspect both proposed and accepted rotated geometry.
