# Layout Benchmark Artifacts

This directory stores normalized diagnostic measurements, not generated layouts or expected solver answers.

## Heavy Xiranite Known-Bad Baseline

`heavy-xiranite-forge.iterative-scc.known-bad.json` records the behavior before the incremental optimizer cutover. Its `baseline_status` is `known-bad-diagnostic-only`. Tests and later implementations must not preserve its coordinates, perimeter routes, or score.

The Heavy Xiranite research checkpoints are documented in
`heavy-xiranite-minimum-rate.first-search-space-report.md` and
`heavy-xiranite-minimum-rate.second-model-structure-report.md`. The third checkpoint,
`heavy-xiranite-minimum-rate.third-bound-sensitivity-report.md`, compares the same exact first SCC
phase under seven square request ceilings. These bounds are diagnostic request ceilings only, not
project defaults or canonical game limits.

The fourth checkpoint, `heavy-xiranite-minimum-rate.fourth-first-incumbent-ablation-report.md`,
uses a temporary release-only ablation build to identify the smallest tested model feature that
crosses the five-second first-solution budget. None of its relaxed variants are production solver
paths.

The fifth checkpoint,
`heavy-xiranite-minimum-rate.fifth-circulation-permitted-remeasurement-report.md`, repeats the
bound series after route acyclicity proof was removed. It includes the current model-family and
first-incumbent breakdowns and identifies the one-to-two-network transition as the next measured
research boundary.

The sixth checkpoint,
`heavy-xiranite-minimum-rate.sixth-network-interaction-decomposition-report.md`, isolates all three
phase-zero networks and pairs. It separates placement-port coupling from multiplicative routing
state and records the newly observed mismatch between circulation-permitted solving and witness
validation.

The seventh checkpoint, `heavy-xiranite-minimum-rate.seventh-route-state-cliff-report.md`, rebuilds
clean network subsets, locates the first cliff at two simultaneously free route-state spaces, and
records the exact cancellation dominance rule for co-located terminals.

The eighth checkpoint, `heavy-xiranite-minimum-rate.eighth-shared-transport-layer-report.md`,
compares the dense per-network routing grid with an exact shared belt/pipe layer formulation. The
shared formulation reduces the recorded 12 by 12 phase-zero model substantially but does not find
a first incumbent within five or fifteen seconds.

The ninth checkpoint, `heavy-xiranite-minimum-rate.ninth-factored-endpoint-report.md`, replaces
flattened placement-times-port endpoint Booleans with exact independent port choices and
solver-native element geometry. It measures endpoint state, placement-routing coupling, build
time, and peak RSS reductions, while recording that the first-incumbent cliff remains.

The tenth checkpoint, `heavy-xiranite-minimum-rate.tenth-factored-network-decomposition-report.md`,
rebuilds every single, pair, and full phase-zero network subset under the factored shared-layer
formulation. It locates the earliest validated-output boundary inside the two-requirement Xiranite
Powder network and separates that boundary from the later one-layer versus two-layer composition
growth.

The eleventh checkpoint, `heavy-xiranite-minimum-rate.eleventh-requirement-decomposition-report.md`,
rebuilds the hard Xiranite Powder network from each logical requirement. It shows that both
requirements are individually tractable and that the first local cliff is the second terminal
pair's placement-port-to-grid coupling, not additional routing-grid state.

The twenty-fourth checkpoint,
`heavy-xiranite-minimum-rate.twenty-fourth-full-phase0-dimension-sweep-report.md`, applies the
parallel exact dimension portfolio to all three phase-zero networks. It proves minimum used area
42 in about 0.8 seconds after the same current free-dimension formulation finds no incumbent in
five seconds, and identifies the used-dimension/boundary-terminal/routing propagation cycle as the
closed phase-zero cliff.

The twenty-fifth checkpoint,
`heavy-xiranite-minimum-rate.twenty-fifth-cumulative-dimension-growth-phase1-report.md`, grows the
same exact portfolio through cumulative SCC phase 1 with a non-binding prior placement hint. It
proves minimum area 77 for the two-facility, six-network graph in 3.65 seconds and moves the next
first-feasible cliff to phase 2 or later.

The twenty-sixth checkpoint,
`heavy-xiranite-minimum-rate.twenty-sixth-cumulative-phase2-cliff-report.md`, locates that cliff at
cumulative SCC phase 2. A 30-second exact dimension portfolio proves every shape through `11x11`
infeasible but leaves `11x12`, `12x11`, and `12x12` unknown with no incumbent, then traces the model
growth to placement/port endpoint geometry and boundary-terminal coupling rather than replicated
routing grids.

The thirty-first checkpoint,
`heavy-xiranite-minimum-rate.thirty-first-phase2-transport-tile-cap-report.md`, adds only an exact
physical transport-tile upper bound to the otherwise unchanged `12x12` Phase 2 first-witness
problem. Caps 48, 64, 80, and 96 all remain unknown at five seconds, and the known-feasible cap 96
remains unknown at thirty seconds. This shows that a monotone tile-budget search is logically exact
but does not by itself break the coupled routing cliff.

The thirty-second checkpoint,
`heavy-xiranite-minimum-rate.thirty-second-phase2-routing-state-breakdown-report.md`, splits the
remaining fixed-placement/fixed-terminal Phase 2 cliff across routing state families, transport
layers, Boolean values, commodity networks, and individual physical cells. It isolates the smallest
observed trigger to one occupied pipe cell immediately downstream of a facility demand terminal and
identifies the terminal-approach/support disjunction as the next exact reformulation target.

The thirty-third checkpoint,
`heavy-xiranite-minimum-rate.thirty-third-phase2-connectivity-witness-report.md`, tests a redundant
exact parent/depth connectivity forest on the same controlled Phase 2 routing problem. The forest
preserves the accepted routing semantics but adds 7,680 variables and 35,920 constraints, and it
repeatedly doubles first-witness time. The checkpoint rejects a dense declarative proof forest as a
performance strategy and identifies global possible-graph propagation as the next connectivity
experiment.

The thirty-fourth checkpoint,
`heavy-xiranite-minimum-rate.thirty-fourth-possible-graph-connectivity-propagator-report.md`, adds a
diagnostic-only Pumpkin custom propagator that removes a demand terminal only when it is unreachable
in the current possible material graph. It adds no decision variables, repeatedly reduces the
controlled Phase 2 first-witness time by about 20%, and identifies coarse wakeups plus full graph
rescans as the next exact implementation bottleneck.

The thirty-fifth checkpoint,
`heavy-xiranite-minimum-rate.thirty-fifth-event-selective-connectivity-report.md`, adds structured
native Pumpkin search statistics and tests exact exclusion-predicate subscriptions. Native counters
show that the broad custom propagator reduces decisions, conflicts, and solver propagator calls,
while more than one million predicate notifications make the event-selective variant slower. The
predicate-watcher variant is rejected, and lazy graph traversal plus lazy explanations become the
next exact optimization target.

The thirty-sixth checkpoint,
`heavy-xiranite-minimum-rate.thirty-sixth-lazy-connectivity-propagator-report.md`, keeps the broad
wakeup schedule and exact search tree while traversing only arcs reachable from possible supplies
and constructing explanations only for unsupported demands. It cuts custom arc scans by 86.5%,
explanation builds by 99.4%, and controlled Phase 2 first-witness time by 13.3%. Demand-option
scanning is the next measured custom-propagator bottleneck.

The thirty-seventh checkpoint,
`heavy-xiranite-minimum-rate.thirty-seventh-grouped-demand-connectivity-report.md`, groups demand
options by physical cell without changing exact semantics. It preserves the complete search tree
but replaces 6.56 million option queries with 19.90 million cell checks and does not improve
runtime. The variant is rejected, and omission of semantically irrelevant demand-only wakeups is
the next exact event-scheduling experiment.

The thirty-eighth checkpoint,
`heavy-xiranite-minimum-rate.thirty-eighth-demand-silent-connectivity-report.md`, proves that demand
selection changes need not wake possible-graph propagation, but removing 2,864 registered variables
does not remove any custom executions on the controlled workload. Its three-iteration summary
accepts lazy traversal as a 13.3% improvement, rejects two demand-side micro-optimizations, and moves
the next research boundary to stronger exact connectivity inference.

The thirty-ninth checkpoint,
`heavy-xiranite-minimum-rate.thirty-ninth-layer-grid-opportunity-report.md`, attaches a passive
analyzer that reads each belt or pipe layer as one directed two-dimensional grid. It preserves the
complete search tree while finding 194 distinct unresolved predicates on 66 unique-support arcs.
The terminal-adjacent subset contains 27 predicates on nine arcs and becomes the first active exact
grid-propagation target.

The fortieth checkpoint,
`heavy-xiranite-minimum-rate.fortieth-terminal-support-grid-propagator-report.md`, implements that
active exact inference. After rejecting a layer-wide 1,085-predicate explanation, it uses a local
physical-adjacency reason of at most nine predicates and targeted execution. Repeated release runs
cut controlled Phase 2 backtracks by 23.5%, native propagations by 8.5%, and median first-witness
time by 9.4%, while preserving independent witness validation.

The forty-first checkpoint,
`heavy-xiranite-minimum-rate.forty-first-unique-support-chain-report.md`, recursively extends the
same exact rule through sole-predecessor route cells. It cuts another 2.6% of backtracks and reaches
23-cell chains, but 176-predicate explanations and broad wakeups leave median runtime flat. The
variant passes Pumpkin reason checking but is rejected as a runtime improvement; selective exact
wakeup scheduling becomes the next implementation target.

The forty-second checkpoint,
`heavy-xiranite-minimum-rate.forty-second-semantic-grid-propagators-report.md`, separates passive
analysis, terminal support, and recursive chain support into semantic-rule-specific Pumpkin
propagators. A support-loss-only schedule preserves the chain's exact search tree but removes none
of its 33,376 executions, so event masks are rejected as the next optimization boundary. Future
work must maintain incremental watched state or add a stronger exact semantic rule.

The forty-third checkpoint,
`heavy-xiranite-minimum-rate.forty-third-dirty-material-chain-report.md`, reruns the unchanged
recursive chain rule only for materials touched by support-loss events. Both release runs reproduce
all 143,766 material passes and the complete broad-chain search tree because route and shared
arm-item events dirty every material on their layer. Material-level filtering is rejected; exact
reverse demand-chain watchers at `(material, cell)` become the next target.

The forty-fourth checkpoint,
`heavy-xiranite-minimum-rate.forty-fourth-watched-demand-chain-report.md`, records every cell read by
each selected demand's exact backward support chain and uses reverse `(material, cell)` dependencies
to recompute only affected demands. It preserves all 43,543 decisions, 8,577 backtracks, 361 forced
predicates, the validated witness, and its objective while cutting grid executions by 32.1%, chain
steps by 66.8%, and two-run first-witness midpoint by 7.5%. The deterministic scheduling work
reduction is accepted; the elapsed values remain fixed-order host-local observations. The next
experiment must strengthen exact semantic inference enough to remove search decisions.

The forty-fifth checkpoint,
`heavy-xiranite-minimum-rate.forty-fifth-local-continuation-analysis-report.md`, passively measures
an exact non-bridge cell-conservation rule beyond the watched-demand chain. After rejecting
bridge-unfiltered preliminary counts, the conservative completed A/B preserves all 43,543
decisions, 8,577 backtracks, the validated witness, and its objective while identifying 133 distinct
unresolved forward support arcs and 94 backward arcs. The broad observer is diagnostic-only because
its full rescans more than double elapsed time; an event-driven active propagator is the next exact
experiment.

The forty-sixth checkpoint,
`heavy-xiranite-minimum-rate.forty-sixth-active-local-continuation-report.md`, activates the exact
non-bridge forward/backward continuation rule with dirty `(material, cell)` scheduling. Against the
watched-demand Phase 2 baseline it cuts decisions by 18.8%, backtracks by 15.3%, and solver
propagations by 20.8% while returning a validated, lexicographically no-worse first witness. The
local rule replaces about 21.46 million passive material/cell checks with 358,572 dirty-key checks.
The next study grows cumulative SCC phases with this rule until a new cliff appears rather than
continuing local micro-optimization in isolation.

Regenerate it from a release binary with:

```bash
python3 tools/capture_iterative_scc_baseline.py \
  --output docs/benchmarks/heavy-xiranite-forge.iterative-scc.known-bad.json
```

The capture tool builds `aic-cli` before starting the timer, executes the exact command stored in the artifact, and commits only normalized metrics. It records input hashes, the source commit, host description, search bounds, independently measured used geometry, logical route cells, route edges, unique route tiles, turns, components, zero-flow routes, and solve time for every phase.

The baseline fixture contract is split across:

- `crates/aic-data/tests/iterative_scc_baseline.rs` for deterministic chain, branch, cycle, and one-facility graphs;
- `tools/tests/test_capture_iterative_scc_baseline.py` for metric normalization;
- the normalized JSON artifact for the release Heavy Xiranite observation.

The one-facility fixture now prevents regression to the captured behavior: every external route is one cell, the canonical witness is identical under 50 by 50 and 500 by 500 search bounds, and reported bounds describe used geometry only.

The first captured phase contains one facility but uses 53 logical route cells: 40 belt cells and 13 pipe cells. The final phase uses 10,338 logical route cells and 523 bridge components. These values are comparison evidence for later slices, not MVP acceptance targets.
