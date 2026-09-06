# Retained-order obstruction diagnostics

Input: a complete incumbent detail and its runtime catalogs. Run the existing fixed-order
compactor independently on each axis, without promoting any candidate. Consume its actual
coordinate/group/difference-constraint certificate, not a duplicate packing model.

For automatically detected interior empty rectangles, identify retained points immediately
on opposing boundaries. For each source group, derive longest-path relative lower bounds
from the certificate inequalities. Original coordinates certify absence of positive cycles.
A tight path certifies only the required separation under these retained order and rigid
geometry constraints. It is not a game-rule separation or a global packing lower bound.
Same-group offsets are exact in this model. Unreachable targets have no path-derived bound;
this does not certify realizable movement or joint route feasibility.

Output JSON includes source witness hash, rectangle coordinates, actual gap, model-required
gap, involved rigid groups, facility labels and network identities. Bound work and emitted
rectangle/pair counts. Failure has a stable stage/code, no fallback or incumbent mutation.
The diagnostic is explanatory only: it neither removes constraints nor accepts unvalidated
geometry. A subsequent coordinated-placement experiment must reconstruct affected interfaces
and pass the whole-factory validator. Pumpkin remains an optional research strategy.

An equality-only ablation additionally reconstructs facility rigid groups from body,
terminal, perpendicular-segment and cross-layer-coincidence ties. Require its coupled
facility partition to match the actual compaction certificate before interpreting the
independent-layer partition. Removing incidental cross-layer ties is a diagnostic relaxation,
not permission to delete body ties or a claim about the resulting factory area.
