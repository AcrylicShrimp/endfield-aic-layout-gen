# Layer Grid Propagation Opportunity Analyzer

## Purpose

Measure whether a custom propagator that understands each belt or pipe layer as one two-dimensional
grid can derive exact routing decisions that the current local formulation leaves unresolved.

This first slice is observation-only. It must not post a predicate, change a domain, add a decision
variable, or alter the production solver path.

## Input contract

One analyzer instance represents one physical transport layer and receives:

- the grid cell count and directed orthogonal route arcs;
- each arc's activation variable and two directional item variables;
- every material code available on the layer; and
- the supply and demand terminal options for each material.

Belt and pipe remain separate layer instances. Facility occupancy is already reflected in the route
and item domains observed by the analyzer.

## First exact opportunity

For each selected demand, construct the current possible directed material graph and start from all
still-possible supply cells. If the demand is reachable and exactly one possible incoming arc from a
reachable cell can support it, every legal completion connecting that demand must use that arc.

The same rule can be followed backward from the predecessor cell until:

- a possible supply cell is reached;
- multiple incoming supports remain;
- no support remains; or
- a repeated cell is detected.

The analyzer records each activation or item predicate on this unique-support chain that is not yet
fixed. It does not enforce the predicate in this slice.

This is a deliberately narrow subset of general grid cut reasoning. Every reported predicate is a
candidate exact deduction; absence of an opportunity is not a completeness claim.

## Output contract

Structured counters report:

- analyzer executions and material passes;
- selected and reachable selected demand options;
- unique-support chain steps;
- unresolved predicate observations;
- distinct support arcs and distinct unresolved predicates;
- maximum observed chain length; and
- registered domain variables.

The controlled benchmark also records the normal solver decisions, backtracks, conflicts, learned
clauses, propagations, witness objective, validation, and elapsed time.

## Invariants

- No solver domain is changed by the analyzer.
- No legal placement, port, terminal, belt, or pipe decision is excluded.
- The accepted possible-graph propagator remains responsible for current connectivity pruning.
- The analyzer is attached only to a diagnostic solve with fixed placement and terminals.
- The normal joint solver path remains unchanged.

## Decision rule

Implement a pruning grid propagator only if the analyzer observes a material number of distinct
unresolved exact predicates before the first witness. If opportunities are rare, stop rather than
building a large custom propagator. If opportunities are common, the next slice must define sound
reasons for the smallest high-value deduction before posting it.
