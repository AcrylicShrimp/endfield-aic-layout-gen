# Mixed-transport factory closure

Status: research stage, not a production architecture cutover.

Inputs: normalized original graph, runtime catalogs and an independently validated concrete partial
block witness. Preserve all retained facilities, ports, segments and rates. Every added facility is
topologically ready: its internal suppliers are already present. Use catalog east-flow belt orientation
and north-input/south-output pipe geometry where available. Select nearby placements from supplier
port coordinates; jointly try unused port pairs and finite orthogonal path templates. Do not invoke
global placement/routing search or substitute external supply for an internal dependency.

This slice embeds remaining small point-to-point obligations into separate capacity-safe ports;
existing split/merge/cross structures remain concrete and unchanged. Separate ports are not a claim
that shared trunks or new junctions can always be avoided. Unsupported port pressure or exhausted
placement/template budgets produce explicit local failures, not an infeasibility proof.

Each private facility candidate includes all its internal and actual external inputs. Validate geometry
and original pending margins before retaining it. Unconsumed original output margins remain pending.
Finally attach only original external outputs. External terminals use the original endpoint IDs and
actual used boundary; never manufacture a new raw input, surplus sink or target. Compare validated
candidate geometry by used area, physical transport tiles and turns; component count is not an objective.

Output: witness v1, original labels/projected ports, frontier trial/route template diagnostics, separate
construction/validation timing and complete-graph coverage. Final success requires all original members
exactly once, original recipes/utilizations, all recipe terminals covered, exact external endpoint/item/
direction margins, no leftover cuts, and independent Rust geometry validation. Factory completeness
must not be inferred from the geometry gate alone. No claim of minimum area or general recipe support.

The generated placement portfolio tries at most the caller's diagnostic trial budget and retains up
to eight geometrically constructed candidates per facility. It is a bounded heuristic neighborhood,
not an exhaustive placement/port optimization. Geometry and exact graph coverage independently gate
every retained candidate. Pipes precede belts within the new facility transaction. Path-template and
port-pair pruning uses only lower bounds on area, Manhattan cell count and zero turns.

Actual external inputs enter on the used north boundary. Final exports may use north, east or south.
The final export transaction reserves one additional coordinate row/column beyond current used
geometry, plans all exports within that shared envelope, then validates their actual boundary position.
This is a chosen candidate reservation, not a caller-supplied factory limit. Failure does not establish
that another envelope or route family is impossible.

Persist every candidate witness, independent validator report and original-graph coverage report.
Publish duplicated detail JSON and rendered HTML only for selected states. The run report contains
candidate hashes, selected paths, failures, generation time, validator process time, input/coverage IO
and selected rendering IO. Full raw candidate diagrams can be regenerated from the retained witnesses
and runtime catalogs; their absence is not deletion of validation evidence.

`--route-family simple` remains the default two-turn corridor family. The explicit `perimeter`
family includes those templates and additionally reserves a transfer row plus a separate entry/exit
column (up to three turns). This can approach a north-facing input around its body after travelling
below other regions. It uses the same occupancy, capacity, exact-score pruning and independent gates;
no extra crossing permission is introduced. The richer local family can still lead to different later
greedy choices, so compare complete factory candidates, not just locally shorter paths.

`--placement-family forward` preserves the original supplier-exit anchor and nonnegative horizontal
offsets. The explicit `two-sided` experiment adds the mirrored negative offsets while keeping the
same catalog orientation, vertical offsets, nonnegative coordinates and body/transport collision
checks. Negative offsets are relative to the anchor, not negative world coordinates or a rotated
production flow. The diagnostic trial budget and eight-success limit still apply after area/distance
ranking. Report the family at run and candidate levels; a larger candidate family is not guaranteed
to improve the complete greedy construction. Retain the complete incumbent in comparative runs.
