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
