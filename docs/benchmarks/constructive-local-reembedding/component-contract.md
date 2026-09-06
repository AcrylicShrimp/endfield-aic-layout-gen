# Runtime component pose replanning

Input: a complete validated detail, runtime catalogs, schema-3 facility moves, and an
optional `componentPoses` array (at most eight unique IDs, integer x/y and catalog rotation).
Only existing one-cell logistics components are supported by this initial family.
Preserve component identity/type, network membership, rates and logical branch allocation.
Do not silently remove junctions or treat a component as an ordinary traversable cell.

Apply explicit component poses in the same final-state transaction as facility moves.
They override implicit translation from the old junction ownership policy. An explicit
original pose pins the component instead of silently allowing downstream translation. Keep
nonterminal ownership and collision checks. Rotate every incident chain attachment arm
by the component's rotation delta. Even an in-place rotation invalidates those chains;
only equal-shift, unrotated interior chains can remain rigid. Restore all displaced and
incident routes before independent whole-factory acceptance.

Propose component poses using actual catalog rotations, legal body/terminal/anchor cells
and incident endpoint distance after facility movement. This is bounded geometric candidate
ranking, not joint route optimality. Test standalone component movement on both the incumbent
and the previously validated non-improving compound alternative, then coupled facility/
component choices against retained-component controls if useful. No Pumpkin dependency.

Diagnostics: stable component identity/catalog/pose/collision failures; attempted and selected
poses, affected chains and work; full area/cells/turns; independent geometry and original-graph
coverage. A rotation preserves the old arm-to-chain allocation; free branch-arm permutation
and logical network resynthesis are outside this initial family. Keep the complete incumbent
on failure or a worse score. Publish and personally inspect actual results and failures.

CLI: `run.mjs --mode component --move-limit 1..512 --rounds 1..8` opts into the
standalone pose portfolio, with the normal required graph/baseline/catalog/validator/output
paths. It does not run fixed-path or facility relocation phases. `--move-limit` bounds
pose proposals per round in this mode. Each proposal changes one existing component;
at most 32 shortlisted poses per component compete for the round limit. Near/far
routing orders and normal full-score/validation gates remain explicit. The API supports
joint facility/component transactions; this CLI slice does not yet propose them.
