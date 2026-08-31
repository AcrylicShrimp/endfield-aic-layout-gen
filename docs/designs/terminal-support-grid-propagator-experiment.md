# Terminal Support Grid Propagator Experiment

## Purpose

Test the smallest active exact inference identified by the layer-wide grid analyzer: force the only
possible directed material arc that can enter a selected demand cell.

The experiment remains diagnostic-only and runs beside the accepted broad-lazy possible-graph
propagator on the fixed-placement, fixed-terminal cumulative Phase 2 `12x12` case.

## Exact inference

For one material and one selected demand terminal:

1. If the demand cell is itself a possible supply, make no inference.
2. Inspect every physical directed arc entering the demand cell.
3. If exactly one incoming arc can still carry the material, force:
   - the arc activation to one;
   - the predecessor-facing item variable to the material code; and
   - the demand-facing item variable to the material code.

No recursive predecessor-chain inference is active in this slice.

The active mode subscribes only to selected demands, supply predicates on demand cells, and arcs
entering demand cells. It does not construct or traverse a layer-wide reachability graph. The
passive analyzer keeps its broader subscriptions and traversal because those are diagnostic data,
not prerequisites of this local inference.

## Explanation

Each forced predicate is explained by:

- the selected demand predicate;
- one current blocking predicate for every other incoming arc; and
- every currently impossible supply option located on the demand cell.

These local predicates prove that the demand has exactly one remaining physical way to receive
the material. No layer-wide reachability facts are needed, so explanations remain small.

## Invariants

- No placement, port, terminal, route, or material option is fixed before solver propagation.
- The propagator only posts a predicate that is necessary in every completion of the current
  solver state.
- Belt and pipe remain separate physical layers.
- The normal joint solver path remains unchanged.
- The witness must pass the existing independent validator.

## Measurement

Compare broad-lazy, passive layer-grid analysis, and active terminal-support propagation using:

- first-witness time;
- branch decisions, backtracks, conflicts, learned clauses, and native propagator calls;
- active grid executions, forced predicates, conflicts, and maximum reason size;
- exact variable and constraint counts; and
- validated witness objective.

Accept only if repeated release measurements show a meaningful search or elapsed-time improvement.
Otherwise retain the analyzer evidence and reject this inference granularity.
