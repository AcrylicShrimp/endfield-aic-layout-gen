# Transactional facility relocation and free-path re-embedding

Status: JavaScript research on a complete independently validated factory. No production cutover.

The current move contract is schema 3 in `compound-contract.md`: a `moves` array of
disjoint rigid translation primitives, with eligible internal junctions. Both the old
absolute-position DTO and the schema-2 single `move` request are rejected. Historical
reports remain evidence at their recorded commits, not compatibility inputs.

Input: complete witness/detail, original graph, runtime catalogs and explicit experiment budgets.
Preserve facility identities, recipes, utilizations and rotations; logical commodity flows,
terminal rates and logistics-component identities remain unchanged. A move changes selected
facility positions and their attached terminal/junction positions. Existing boundary route shapes
are not constraints; wholly internal group chains translate rigidly in the group family.

Extract directed constant-rate chains between terminals, logistics components, shared crossing
cells, branch/merge vertices and rate changes. Bends are not anchors. Preserve directed attachment
arms at chain boundaries. Remove each selected chain interior and find a path in the unit grid
with A*, minimizing length first and turns second. Facility bodies and retained same-layer cells
are obstacles; independent transport layers remain independent. No new crossings or shared trunks
are invented by this slice. Bound the search by the current witness extent, not a game canvas.
Failure within that neighborhood is not global infeasibility. Report exhausted work separately.

A facility move rips up every chain touching its new body or translated attached terminal, as
well as every incident chain. Reject overlap with other bodies or fixed anchors/components.
Re-embed affected chains transactionally in deterministic order. All removed chains must be
restored with exact rates; otherwise discard the whole candidate. Ordinary fixed-placement
rerouting is a control, not a substitute for relocation. Candidate ordering uses endpoint-distance
change only as a heuristic, not as proof or final acceptance. Search budgets are explicit.

Output: detached candidate detail, relocated runtime ports, invalidated acceptance flags,
affected chains, path search work/timing and stable rejection diagnostics. Validate the complete
candidate independently against both geometry and original graph coverage. Only full score
(used area, physical tiles, turns) improvement replaces the incumbent. Test optional fixed-order
compaction after changed embedding to expose the new order; it is postprocessing, not the primary
placement strategy. Preserve the complete incumbent on all errors.

Verification: exhaustive small-grid shortest paths, directed endpoints, obstructions, budgets,
chain identity/rate conservation, displacement of intersected wires, input immutability, unrelated
region preservation, actual complete-graph validation and negative controls. Inspect full and
same-region before/after renders; distinguish topology-required returns from avoidable detours.
This local greedy sequence makes no joint path, packing or global optimality claim.

The optional terminal-flexible endpoint policy releases old path arms at ordinary singleton
terminals, as specified in terminal-contract.md. It does not rotate facilities or change ports.

Current restrictions: terminal port identities and boundary positions are fixed during routing;
zero-length connections reject relocation when only some endpoint owners move. Single-facility candidate
positions use facility-contact and endpoint-alignment coordinates, prefer lower endpoint-distance
sum and are diversified across facilities. Thus that family can miss area-improving moves that
temporarily worsen the proxy. The group family additionally targets measured empty rectangles
and retains candidates with increasing endpoint distance. Fixed attachment arms and the first/last adjacent cells of other affected
chains are reserved. These are conservative search choices, not additional game rules. A shortest
direction-state walk that repeats a cell is rejected with its own diagnostic, not called infeasible.
Unchanged paths skip optional compaction. Exhaustion means only this bounded greedy neighborhood.
