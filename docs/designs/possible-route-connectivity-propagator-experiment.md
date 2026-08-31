# Possible-Route Connectivity Propagator Experiment

## Question

Can one exact global graph check reduce the controlled Phase 2 first-witness cliff without adding
another routing certificate or removing any legal route?

## Scope

The first custom Pumpkin propagator is diagnostic-only. It is enabled only by the dedicated
research comparison and remains disabled in production solving.

It introduces no placement, port, route, parent, depth, path-candidate, or objective variables.
It does not select a route. It reads the domains of the existing exact variables.

## Possible graph

Build one directed over-approximation for each material network from the current solver domains.

A physical directed arc is present in the possible graph when:

- the physical arc can still be selected;
- its source arm can still carry the network item; and
- its destination arm can still carry the network item.

Every supply-terminal option that can still be selected is a possible graph source. A
demand-terminal option is supported when its cell is reachable from at least one possible source.

Cell traversal deliberately over-approximates bridge topology in this first experiment. It may
retain an impossible connection through a crossing, but it must never remove a physically legal
connection. This can weaken propagation but cannot exclude a legal solution.

## Propagation

For every demand-terminal option that is not reachable in the possible graph, post that the option
cannot be selected.

The explanation contains:

- every supply option currently excluded from the possible source set; and
- one currently true exclusion predicate for every physical arc absent from the possible graph.

If a selected demand is unreachable, the posted exclusion produces a solver conflict with the same
reason.

This first slice does not force bridge or cut arcs. Mandatory-cut reasoning is a separate possible
strengthening after the basic propagator is proven sound and measured.

## Exactness argument

Every real route for a material uses only selected physical arcs whose two incident arms carry that
material. Therefore every real route is also a path in the possible graph.

If no possible-graph path reaches one demand option, no completion of the current domains can route
that demand option from a compatible supply. Removing only that terminal option is sound.

The possible graph may contain extra paths because it over-approximates unresolved topology. Extra
paths can delay a propagation but cannot create an unsound propagation.

## Controlled comparison

- Heavy Xiranite minimum-rate cumulative SCC Phase 2;
- exact used dimensions fixed to `12x12`;
- all facility placements and all facility/external terminals fixed to the same validated
  reference used by the preceding connectivity-witness experiment;
- placement, item, flow, topology, capacity, and collision constraints otherwise unchanged;
- feasibility-only search with equal release-mode budgets;
- baseline versus possible-graph propagator, with no parent/depth forest.

Record model size, construction time, search time, first incumbent, validation, objective, and
propagator counts. Timeouts remain `unknown`.

## Small-model acceptance

Tests must show that the propagator:

1. removes a demand option after all graph paths to all supplies are excluded;
2. retains the demand while at least one possible path remains;
3. accepts a valid external route with the same objective as the unchanged exact model;
4. adds no solver decision variables.

## Stopping rule

Report the controlled comparison before enabling the propagator in production or adding
mandatory-cut propagation, incremental graph maintenance, custom branching, or routing heuristics.
