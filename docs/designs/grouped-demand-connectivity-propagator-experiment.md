# Grouped Demand Connectivity Propagator Experiment

## Question

Can the exact broad-event lazy connectivity propagator avoid inspecting every demand option on
every execution by grouping options that occupy the same physical cell?

## Exact contract

The diagnostic variant keeps the accepted broad-lazy propagator's:

- possible material graph and supply reachability definition;
- broad integer-domain wakeup schedule and propagation priority;
- demand-option selection variables;
- unsupported-demand exclusion and explanation predicates; and
- placement, port, terminal, and routing freedom.

Grouping is a construction-time index only. It does not merge demand variables or remove an
option. If a cell is unreachable, every still-selectable demand option attached to that cell is
processed. If a cell is reachable, all its options are supported by the same possible graph and
their individual domains do not need to be queried.

## Controlled comparison

Extend the fixed-placement, fixed-terminal cumulative Phase 2 `12x12` diagnosis with a fifth case:

1. unchanged baseline;
2. broad-event eager connectivity;
3. rejected event-selective eager connectivity;
4. broad-event lazy arc traversal; and
5. broad-event lazy arc traversal with demand options grouped by cell.

Record native search statistics, custom executions, arc checks, demand cells checked, demand
options checked, explanation work, pruning, conflicts, validation, and witness objective.

## Acceptance and stopping rule

The grouped case must keep the exact variable count and produce a validated witness. When the same
search tree is observed, it is strong evidence that this is a pure internal-work optimization. If
posting order changes the search tree, accept the variant only if the complete legal solution set
and explanation contract remain unchanged and repeated release measurements show a clear runtime
benefit.

Report before changing wakeup policy, explanation strength, branching, or production enablement.
