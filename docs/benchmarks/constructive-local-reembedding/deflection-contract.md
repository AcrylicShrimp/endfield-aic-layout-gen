# Boundary translation with a separately relocated member

## Scope

This bounded improvement operator acts on a complete validated witness. It creates
two simultaneous translation primitives: an inward boundary group and one member
relocated independently into a current empty rectangle or a nearby cardinal pose.
It is not construction, a global placement phase, or an exact packing solver.

`deflectionCandidates(detail, catalogs, options)` accepts the existing witness DTO
and runtime catalogs. Options bound selected candidates (1..512), examined combined
poses (1..65536), inward steps (1..8) and total moved members (2..16). Defaults are
128 candidates, 16,384 poses, four inward steps and sixteen members. Junction policy
is the existing internal/downstream policy. No recipe, label or game size is special.

## Proposal and validation contract

Derive the existing one-through-four-cell body-envelope cohorts for all four sides.
For each inward displacement, close body and terminal contacts using the rigid push
closure. Deduplicate closed translations. A rejected rigid closure is not reconsidered
by this operator. For each member of each closed group of at least two facilities,
propose separating that member from the group's translation. Consider corners of up
to eight current empty rectangles that fit its body, then interleave nearby cardinal
translations through eight cells. The voids include current wires/components in their
occupancy; neither this list nor the combined pose family is exhaustive.

Interleave per-group/member streams so early groups cannot consume every pose. Count
each sampled combined pose before screening. Preserve rotations, original terminals,
and external obligations. Combined geometry is evaluated simultaneously: one facility
cannot occur in both primitives, and non-detachable shared-terminal owners still need
the same translation. Reject collisions rather than silently enlarging or deforming
the second group. No routing or group-budget relaxation is introduced.

Release the union of incident bridges and bridges covered by proposed moved bodies,
up to the existing eight-release limit. Cache immutable chain indexes by release set;
the cache is local to this invocation and is not a physical witness. Apply the existing
movement checks to the exact released draft and reject carried geometry leaving the
current extent. This fixed extent only defines this improvement neighborhood.

Rank geometry-screened proposals by used body-envelope area, endpoint-distance delta
and deterministic origin, with round-robin depth across group/member pairs. The body
envelope is only a proposal proxy. The runner reroutes affected chains, tries its
existing bounded vacant-port repairs, compacts independently by layer when requested,
and independently checks the whole factory before accepting a score improvement.
The authoritative score remains actual used area, transport cells, then turns.

## Reports, failures and verification

Return candidates with both moves, rigid closure, original boundary seed, separated
member, release IDs and ranking proxies. Report boundary/closed-group counts, voids,
sampled/budgeted poses, geometry rejections and bounded diagnostic examples. Diagnostics
use stable codes with severity, stage, message and entity. Bad options use
`deflection-invalid-input`; body/carry extent and release exhaustion have explicit
`deflection-bounds` and `deflection-bridge-budget` codes. Existing movement failures
retain their codes. No rejected/truncated neighborhood proves global infeasibility.

CLI `--move-family deflect` requires move/joint mode and bridge release. Other families
remain explicit controls. Tests cover two disjoint primitive membership, independent
member motion, body collisions, deterministic bounded sampling, invalid options and
immutability. Run matched same-incumbent push controls, full graph validation, native
full/focused inspection and workspace checks before selecting a new factory.
