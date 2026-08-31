# Phase 2 Connectivity Witness Experiment

## Question

Does an explicit source-to-demand connectivity proof remove the remaining Phase 2 first-witness
cliff that the current local material-flow formulation does not propagate through within five
seconds?

## Controlled baseline

- Heavy Xiranite minimum-rate cumulative SCC Phase 2.
- Exact used dimensions fixed to `12x12`.
- Every facility placement and every facility/external terminal fixed to the same independently
  validated reference used by the routing-state breakdown.
- All physical belt/pipe cells, arms, arcs, item assignments, integer flows, and logistics
  components remain solver decisions.
- Feasibility-only Pumpkin search with an equal wall-clock budget.
- The connectivity witness is diagnostic-only. Production solver behavior remains unchanged.

## Witness contract

Create one proof forest per commodity network over the existing directed grid arcs.

For each commodity network and grid cell, model:

- whether the cell belongs to the proof forest;
- whether a selected supply terminal makes the cell a proof root;
- at most one selected incoming proof-parent arc;
- a bounded proof depth.

The exact constraints are:

1. every selected demand terminal cell belongs to the proof forest;
2. every selected supply terminal cell is a proof root;
3. a proof root has no incoming proof parent and depth zero;
4. every reached non-root cell has exactly one incoming proof parent;
5. a proof-parent arc implies the corresponding real directed route arc is active;
6. a proof-parent arc implies both route arms carry the witness commodity;
7. a proof-parent arc reaches both endpoint cells and increases depth by exactly one;
8. an unreached cell has depth zero.

Proof-parent arcs are a subset of the real routing arcs. The real routing network may contain
additional branches, convergences, shared trunks, bridges, or cycles.

## Equivalence argument

The witness must preserve the existing feasible set.

- Any witness-augmented solution is an existing solution because all original placement, routing,
  item, capacity, topology, and material-flow constraints remain present.
- Any existing valid material flow can be decomposed into supply-to-demand paths plus
  circulations. Select one positive-flow path to each demand, remove proof cycles, and orient one
  parent per reached non-root cell. This constructs the required forest without changing the real
  route.
- Multiple supplies remain independent roots. Each demand may reach any compatible supply in the
  same commodity network. Original logical edge pairs are not imposed as mandatory physical paths.

## Evidence

Record for baseline and witness cases:

- construction time, search time, first incumbent time, and outcome;
- full objective vector and witness validation;
- variable and constraint counts by family;
- connectivity parent, reachability, root, and depth state;
- factor-graph incidences and placement-routing incidences;
- machine-readable JSON and self-contained HTML, including failures.

## Acceptance

- Small controlled fixtures accept the same representative feasible routing behavior with and
  without the redundant witness.
- The witness case never fixes a placement, terminal, route cell, arc, flow, item, or component
  beyond the common controlled baseline.
- A timeout remains `unknown`.

## Stopping rule

Run the controlled Phase 2 comparison and report the result before changing the production
formulation. If the declarative forest adds cost without improving the first witness, evaluate a
custom graph-connectivity propagator separately rather than adding a route heuristic or path
candidate generator.
