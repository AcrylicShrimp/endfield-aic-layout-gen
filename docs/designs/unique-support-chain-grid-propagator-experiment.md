# Unique-Support Chain Grid Propagator Experiment

## Purpose

Test whether the accepted terminal-adjacent grid inference can propagate farther into a route
without a layer-wide reachability proof or a large global explanation.

The experiment is diagnostic-only. It runs beside the broad-lazy possible-graph propagator and the
terminal-only grid propagator on the fixed-placement, fixed-terminal cumulative Phase 2 `12x12`
case.

## Exact inference

For one material and one selected demand terminal, maintain a required cell starting at the demand
cell:

1. If the required cell can still be a supply terminal for the material, stop.
2. Inspect every physical directed arc entering the required cell.
3. If zero or more than one incoming arc can still carry the material, stop.
4. If exactly one incoming arc can carry the material, force that arc's activation and both arm
   item values.
5. Move the required cell to the forced arc's predecessor and repeat.

The walk stops if it revisits a cell. It never chooses between multiple possible predecessors.

## Explanation

The reason starts with the selected demand predicate. At each required cell it accumulates:

- one current blocking predicate for every alternative incoming arc; and
- every currently impossible supply option located on that cell.

For the first forced arc, these predicates prove that the demand has exactly one possible source
direction. For every later forced arc, the accumulated suffix predicates prove that every route
from a possible supply to the selected demand must enter the current required cell, and the new
local predicates prove that the forced arc is its only possible predecessor.

The explanation grows with the forced chain rather than with the full grid. On a degree-four grid,
each additional step contributes at most three blocked incoming arcs plus local supply blockers.

## Invariants

- No placement, port, terminal, route, or material option is fixed before solver propagation.
- Multiple possible incoming arcs remain solver decisions.
- An incoming arc is not discarded merely because its predecessor is currently unreachable.
- Belt and pipe remain separate physical layers.
- The normal joint solver path remains unchanged.
- Every witness must pass the existing independent validator.

## Measurement

Compare broad-lazy, terminal-only, and recursive-chain cases using:

- first-witness time;
- branch decisions, backtracks, conflicts, learned clauses, and native solver propagations;
- forced predicates, maximum forced chain, and maximum explanation size;
- exact variable and constraint counts; and
- independent witness validation.

Accept only if the chain case reduces the terminal-only search tree and its repeated elapsed-time
cost is not worse. Otherwise retain terminal-only propagation as the measured boundary.

