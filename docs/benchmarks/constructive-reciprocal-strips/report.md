# Discovered alternating reciprocal strips

## Result and scope

Runtime graph inspection identifies seven reciprocal cores with two producers and one
returner. Enumerating facility order, allowed rotation and touching-strip axis discovers
the user-described alternating orientation without any facility-name or item-name rule.
All seven cores have independently validated 15x7 or 7x15 geometry (105 area), eight
physical belt cells in the retained-allocation control and zero logistics components.
Compatible supply reallocation improves this to six belt cells and three connections
at the same 105 area. Every connection has two cells.

The selected example is F51/F56/F50: producer/returner/producer, north/south/north.
The retained-allocation control has one plant and one seed connection at each seam.
Following further user feedback, the selected allocation has one producer supplying all
plant demand to the returner; the other producer supplies the downstream processor.
The returner still feeds seeds to both producers. Thus there are three internal links,
not four. Port assignment chooses distinct compatible corner ports.
No long return perimeter, crossing, splitter or converger is necessary inside this core.
One producer now has the entire outgoing plant demand pending for the downstream
processor. Its total demand is unchanged. No obligation is replaced by a fake external node.

This is a three-facility cyclic core, not the previous four-facility cycle-plus-processor
block and not a completed factory. Do not compare their areas as equivalent coverage.
The previously validated complete 59-facility incumbent remains 73x54 / 3,942 area.

## Method and evidence

For each supported core, enumerate 6 member orders, 4 allowed rotations per member,
and both touching-strip axes: 768 geometric placements, of which eight admit the
two-cell, independent-port connections. Seven cores therefore test 5,376 placements
and yield 56 retained-allocation candidates. All 56 pass the independent release geometry
validator and exact recipe/pending graph-margin checks (retained-matrix.json).

The final portfolio adds two compatible exclusive-supply allocations for every core.
Each preserves the complete graph's per-producer supply and per-consumer demand for
the commodity, including consumers outside the strip. It replaces four quarter-rate
forward fragments with two half-rate supplies while leaving both seed returns intact.
All 168 candidates validate; the selected seven use three links/six belt cells and one
pending downstream output. matrix.json includes both the retained control and alternatives.
Selected details persist the exact replacement-edge delta required for full integration.

Port assignment backtracks over compatible adjacent connection-cell pairs and rejects
body collisions, reused ports, shared same-layer cells and rates above runtime capacity.
The first feasible assignment is kept per layout; no global port/placement optimum is
claimed. An explicit assignment budget produces an incomplete-neighborhood diagnostic.

The three-producer/two-returner core is reported as strip-member-bound, not failed
physics or global infeasibility. A larger alternating sequence alone will not necessarily
satisfy its current cross allocations; decomposition or compatible allocation changes
must retain shared suppliers, every recipe rate and every outgoing obligation.

The retained-only CLI generation takes 1.105s and full elapsed time 1.754s on this machine.
The final allocation portfolio takes 5.433s generation and 6.924s total, including 168
independent validations with four workers and seven selected render files; Node tests
were running concurrently. These are one-run observations, not speed comparisons or
performance guarantees. Two retained-only runs produce the exact same 56 witness hashes.
An initial smoke test omitted the required witness
schema_version and failed deserialization; this was fixed before verified runs.

96 related Node tests pass, including complete rotation/order enumeration, runtime
capacity rejection, renamed facility IDs, input immutability, detached candidates,
two-cell/unique-port checks, whole-graph commodity-margin preservation during reallocation,
three-link selection and rejection of hidden pending margins, changed utilization
and missing terminal reports. cargo check --workspace passes; cargo test --workspace
passes 524 tests with two existing ignores. No Rust files or dependencies changed.

## Visual check and next gate

Personally inspected the native rendered example. It matches the intended touching
producer/returner/producer arrangement, with short loops above and below the two seams.
The selected allocation has one plant link and two seed links, exactly the user's second
observation. The page reports one pending output terminal and partial graph scope.
Six viewport/theme checks pass for both the retained and exclusive-supply images.

Images: target/shared-results/bound-interconnect-reciprocal-strip.png (four-link control),
target/shared-results/bound-interconnect-reciprocal-exclusive.png (three-link selection).

Interactive result: http://100.114.24.86:8765/bound-interconnect-reciprocal-exclusive.html

Next: attach the downstream processor to these compact cores while preserving accessible
output ports, then compose the full graph and compare complete area. Expand the five-member
case with measured compatible-flow decomposition, not by assuming every reciprocal graph
is an independent ABA strip. Keep existing validated whole-factory generation stable.

This experiment constructs a routed atomic cycle privately and validates its covered
subgraph, consistent with the constructive planner design. It does not introduce a
production facility-only placement phase. Its rotation freedom is scoped to reciprocal
cores; ordinary downstream flow is not globally randomized.
