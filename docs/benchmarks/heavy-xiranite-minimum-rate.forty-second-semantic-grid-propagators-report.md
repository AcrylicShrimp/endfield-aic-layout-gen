# Heavy Xiranite Minimum-Rate Semantic Grid Propagators Report

## Result

The grid experiment now uses one Pumpkin propagator type per semantic rule:

- passive layer opportunity analysis;
- terminal unique-support propagation; and
- recursive unique-support-chain propagation.

The propagators share immutable grid data and reporting helpers, but each owns its propagation
entry point, inference label, subscriptions, and proof rule. This removes the previous runtime mode
switch from one monolithic propagator.

Applying support-loss-only wake events to the chain rule does not reduce its executions. Across two
release runs, broad and selective chain variants have exactly the same:

- 43,543 branch decisions;
- 8,577 backtracks, conflicts, and learned clauses;
- 7,081,503 native solver propagations;
- 33,376 grid-propagator executions;
- 143,766 material passes; and
- 361 forced predicates with a maximum 23-cell chain.

The selective schedule is rejected. It preserves exact behavior but removes no work on this
workload. The equal execution count is consistent with discarded events being coalesced with other
subscribed events, but this experiment does not record event-kind notifications and therefore does
not prove that causal explanation.

## Repeated Comparison

| Metric | Terminal A | Broad chain A | Selective chain A | Terminal B | Broad chain B | Selective chain B |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| First witness | 3,595 ms | 3,627 ms | 3,617 ms | 3,541 ms | 3,595 ms | 3,590 ms |
| Branch decisions | 43,545 | 43,543 | 43,543 | 43,545 | 43,543 | 43,543 |
| Backtracks | 8,806 | 8,577 | 8,577 | 8,806 | 8,577 | 8,577 |
| Native solver propagations | 7,219,908 | 7,081,503 | 7,081,503 | 7,219,908 | 7,081,503 | 7,081,503 |
| Grid executions | 33,292 | 33,376 | 33,376 | 33,292 | 33,376 | 33,376 |
| Material passes | 149,626 | 143,766 | 143,766 | 149,626 | 143,766 | 143,766 |
| Forced predicates | 90 | 361 | 361 | 90 | 361 | 361 |
| Validation | passed | passed | passed | passed | passed | passed |

The selective chain's median first-witness time is 3,604 ms, compared with 3,611 ms for broad
chain and 3,568 ms for terminal-only. The seven-millisecond broad/selective difference is noise in
the absence of any execution or search-tree change.

## Exact Wake Contract

The selective chain subscribes only to events that can create a new sole predecessor:

- a demand selection lower-bound increase;
- a route-activation or local-supply upper-bound decrease; and
- any route-arm item-domain change.

It ignores route activation lower-bound increases because selecting a still-possible arc cannot
remove another predecessor. A controlled normal-runtime test verifies exact execution-count
stability after arc selection and a wakeup after demand selection. The separately feature-gated
Pumpkin proof-check run permits Pumpkin's own additional scratch check while continuing to validate
all forced predicates and zero-support conflicts.

## Interpretation

The result closes another local optimization path. Event masks alone cannot reduce scheduling when
many related route domains change in the same solver propagation wave. A useful next implementation
must reduce work inside an execution, for example by maintaining watched supports or dirty
material/cell frontiers, or add a different exact semantic rule that removes substantially more
search.

The per-rule propagator split is retained because it isolates those future experiments. The
support-loss-only schedule remains diagnostic evidence but is not an accepted performance variant.

## Independent Review

An independent propagator review found no missed forward wake event, unsound inference, invalid
recursive explanation, or semantic coupling between the three rule-specific propagator types. It
did find two evidence-quality defects in the first draft:

- the controlled wake test allowed one extra execution under Pumpkin's optional debug checker
  while the draft described an exact zero-wakeup observation; and
- the draft presented event coalescing as the measured cause of the equal execution counts even
  though event-kind notifications were not recorded.

The test now requires exact execution-count stability in the normal runtime and isolates the
debug-checker's additional scratch execution behind an explicit Cargo feature. The report now
states only the observed equality and treats coalescing as an unproven possible explanation.

The reviewer also confirmed that the remaining cost is inside each execution: 33,376 executions
perform 143,766 material passes and 303,473 unique-support-chain steps, with explanations growing
to 176 predicates. The highest-ranked exact follow-up is therefore a watched-support or
dirty-frontier chain propagator that preserves the same deductions while revisiting only demands
and predecessor chains touched by changed cells.

## Artifacts

- `heavy-xiranite-phase2-selective-chain-wakeup-5s/summary.json`
- `heavy-xiranite-phase2-selective-chain-wakeup-repeat-5s/summary.json`
