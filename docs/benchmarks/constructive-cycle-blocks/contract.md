# Recipe-derived cyclic production blocks

Status: experimental, before full-factory composition. No production cutover.

## Input and stage contract

Read the full frozen facility/requirement graph plus runtime catalogs. Ignore its placement,
rotation and port choices. Preserve original facility and requirement identity. Derive utilization
from exact graph terminal margins and require every input/output to match its runtime recipe;
reject missing, extra or imbalanced margins before geometry.

Find strongly connected components. The first supported cycle has two commodities, single-input /
single-output recipes in the SCC, one forward product and a return commodity confined to the SCC.
Include immediate forward-product consumers only when all their ingredients come from this SCC.
Each facility has exactly one owner. Reject unsupported cycle shapes explicitly, without claiming
global infeasibility or silently removing graph requirements.

Construct the forward two-bank interface with the common recipe-derived engine. Replace its
temporary return-commodity external network with a fully internal return network: ordered capacity
buckets, three-way split/merge modules and nested perimeter U paths. Plan this geometry before
lowering unit-grid cells. Return paths do not use A* or hidden unlimited canvas search.
Compare expanding leaves before the perimeter against retaining source capacity buckets through
the perimeter and splitting only afterward. This reduces long parallel return paths without
relaxing rates or treating junction count as an objective.

Remove temporary external networks for outgoing block products; retain those precise full-graph
edges as pending output obligations, never fake external sources/sinks. No actual graph external
obligation is accepted as complete until its real boundary route exists. The witness covers both
directions inside the cycle, all facility inputs, and leaves only the declared outgoing products
pending. It does not model game startup inventories; this is the existing steady-state contract.

## Output, invariants and validation

Detail: witness v1, labels, original graph members, original covered/cut requirement IDs, exact
allocated commodity margins, return port groups, geometry plans, pending obligations and timing.
The full graph inventory distinguishes supported blocks, unsupported SCCs and unowned nodes.
Global factory validation always remains false at this stage.

Independent Rust validation must accept the covered geometry. Additionally require: no external
terminals, all recipe inputs fully covered, and pending outputs exactly equal the declared cut
edges. Original pair flows may be redistributed only within a compatible commodity while preserving
every instance/direction/item margin. A completed local cycle is not a completed factory.
`inspectBlockCoverage` returns acceptance plus structured diagnostics; the CLI persists these in
`coverage.json` and the candidate report. It checks exact membership, immutable recipe/utilization
snapshots and the full set of expected recipe terminal rows against runtime catalogs. A rejected
graph coverage check cannot be hidden behind a successful covered-geometry validator result.

Compare raw and strip-compacted candidates by area, physical tiles, turns. Component count is not
an objective. Keep failures and actual rendered geometry. Use explicit diagnostic codes, severity,
stage, message and entity for graph mismatch, unsupported cycle, port/approach failure, generation
budget and validation rejection. W/H are measured outputs, not feasibility constraints.

CLI flags explicitly identify the graph, five runtime catalogs, release validator and fresh output
directory. Detailed DTOs, failures and validator streams go to files, with a short stdout completion
summary. Up to four candidate validators run concurrently. Report graph preparation (including
graph file read), inventory JSON I/O and geometry construction/compaction separately. Catalog load
and compilation are excluded; `elapsed_ms` includes intermediate output and validation, but precedes
final report serialization. Candidate detail construction time is one family including its forward
interface construction; the total tries both families independently.

## Completion evidence for this slice

Generate and independently validate every supported actual heavy-xiranite SCC block. Include the
larger 3-producer / 2-return-producer cycle and the smaller repeated cycles without label/recipe
hardcoding. Verify negative margin, boundary and broken-return controls. Inspect actual rendered
small and large cycles. Report remaining full-graph nodes and obligations explicitly.
