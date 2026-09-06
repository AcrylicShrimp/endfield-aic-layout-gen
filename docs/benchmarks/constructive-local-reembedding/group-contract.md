# Connected-group translation and interface reconstruction

Move DTO version 2 replaces the single-facility absolute-position request with
`{ instances: string[], dx: integer, dy: integer }`. Single-facility candidates use the same
contract; the former `instance/x/y` DTO is rejected explicitly. Existing recorded reports are
historical evidence, not executable compatibility inputs.

Translate selected facilities rigidly, preserving rotations, recipes and utilization. Carry
terminal anchors and any nonterminal junction component whose complete terminal neighborhood
belongs to the selected group. A boundary-connected junction stays fixed. Shared zero-length
facility connections can move when every endpoint owner moves together. Translate internal
constant-rate chains rigidly when both anchors move; reroute boundary chains and any unrelated
chain intersecting the translated body or retained internal geometry. Preserve all unaffected
geometry. Validate the complete result before acceptance; collision or failed reconstruction
discards the private transaction. No relocation of an external endpoint is part of this slice.

Group candidates come from original connected pairs, short serial chains and discovered cyclic
blocks. Candidate translations use body-contact coordinates and existing grid extent. Screen
actual bodies/anchors, not empty module bounding boxes. Rank geometry by retained-body envelope
and boundary-connection distance, with a separate inward/packing shortlist so increasing wire
length does not exclude potential area improvements. Explicit group/position/work limits are
research bounds; neither a failure nor a plateau proves factory infeasibility or optimality.

Measure fixed-interior preservation, affected boundary/displaced chains, copied junctions,
generation and validation costs. Compare whole-factory area with the complete incumbent; never
substitute sum of group areas. Test translated zero-length terminals, mixed boundary ownership,
internal junctions, displaced wires, nonmutation and actual complete recipe closure. Inspect the
large void and displaced process links in actual before/after renders, not merely summary scores.

## Dependency-cone extension

The opt-in `--move-family cone` retains the existing group proposals and adds bounded
upstream/downstream dependency cones. Starting from each graph node, add complete BFS
layers of internal predecessors or successors, at most four layers and sixteen members.
Never truncate a layer to meet the budget. Deduplicate member sets; external endpoints
are not facilities and are excluded. This is a search neighborhood, not a graph rewrite
or a claim that all selected nodes form an atomic production SCC.

The move DTO and physical movement contract are unchanged: translate eligible internal
geometry, rip up boundary/displaced chains, restore all rates and validate the complete
factory before accepting an area/cell/turn improvement. Preserve the old `group` family
as an explicit experimental control. Record the selected family and proposal kinds;
compare the same complete incumbent and inspect the resulting void and crossing geometry.

The opt-in `--junction-policy downstream` may also carry an entire connected nonterminal
anchor cluster when every outgoing boundary terminal is an input of a selected facility.
At least one such outgoing terminal is required. Every moved external-supply connection
is rerouted; no external terminal moves. Otherwise retain the cluster. Preserve the
`internal` control (all adjacent terminal owners must be selected). The cluster includes
all joined bridge arms and rate-change anchors, not an arbitrary subset of a component.
Its geometry and component rotation translate rigidly. This is a conservative directed
ownership heuristic, not proof that other junction poses are impossible.

For rejected moves, the native viewer may receive `move_diagnostic` alongside the
unchanged validated witness: `instances`, integer `dx/dy`, a diagnostic `code` and
`failed_endpoints` in the proposed coordinates. Red dashed bodies and red endpoint rings
are a proposal overlay, not an accepted or partially routed factory. The base witness's
validation must not be confused with acceptance of the highlighted proposal.

Replay requests may supply `routePriority`, an ordered list of unique known chain IDs.
Only chains already selected for rerouting are affected; this list neither deletes chains
nor expands the rip-up set. Priority precedes the near/far heuristic, enabling bounded
failure-first order probes. The default empty priority preserves existing ordering.
