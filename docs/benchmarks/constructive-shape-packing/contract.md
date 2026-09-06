# Joint source-shape packing experiment

Inputs: normalized recipe graph, runtime catalogs, and 1..8 retained region banks.
Each bank contains concrete alternative witnesses with identical facility ownership,
recipe/utilization margins and compatible internal allocation. Alternatives are not
selected solely by their individual area. A source report and every selected region
must be independently checked against the current graph before a factory is promoted.

Pack actual facility, belt, pipe and component cells. Different transport layers may
overlap; bodies and same-layer geometry may not. Preserve enough unused east-facing
output ports for each pending per-item margin, with an unobstructed east ray. This is
an explicit restriction of the existing structured interface, not a game rule or a
proof that other port approaches cannot work. Unused ports without an obligation do
not reserve space. Required lane counts aggregate compatible fragments by item.

Use a bounded beam over next-region choice, shape and integer translations in the
current contact neighborhood. Keep distinct envelope dimensions before filling the
beam with ties. This is heuristic diversity, not dominance pruning or optimality.
Report tested translations, collisions, access rejection, beam truncation, candidate
shapes and construction time. Budgets are experimental search limits, not factory
dimension limits. No caller-supplied factory canvas is introduced.

Output private packing candidates (region choices, normalized integer translations,
partial scores and search evidence). Compose and route the next frontier through the
shared engine using those explicit translations. Independently validate the resulting
covered subgraph and eventually the entire factory. Compare actual whole-factory area,
physical transport cells, then turns; retain the complete incumbent on failure/loss.

The shelf control stays available as a distinct experimental family. The native renderer
is unchanged. Inspect both the best new candidate and complete incumbent if different.
Do not claim that a smaller packed source bank proves a smaller completed factory.

## Measured growth-boundary control

`--growth-fill N` optionally cuts later structured growth when the next fully routed
candidate has occupied-XY/bounding-area below N (0..1; default 0 leaves growth unchanged).
Retain the previous independently validated region, with its pending margins intact;
defer the candidate frontier to closure after region packing. Record both fill metrics,
the rejected growth candidate and the deferred frontier. Physical belt/pipe overlap counts
once for fill, while the transport objective still counts each physical layer separately.
Facility occupancy is reported separately so extra wiring cannot masquerade as productive
density. Fill is a stopping-policy experiment, not the optimization objective.

This initial control operates after the first shared-consumer interface joins each source
bank, preserving the existing two-cluster outer experiment. It does not yet find arbitrary
SCC-seeded group boundaries, require that the first join meet N, or implement two-step
lookahead. Those limitations must be stated in the result; a threshold is not a game rule.
