# Phase 3 External Boundary-Side Exact Partition

## Purpose

The sparse boundary-key A/B removes every table-unsupported external key but leaves residual tuple
case 6 `Unknown`. Each of ten external selectors still has 54 legal root values. This experiment
asks whether choosing only one coarse boundary side for one semantically simple external terminal
is the next first-feasible cliff.

## Parent Control

Reproduce the accepted external boundary-key A/B and require its interpretation gates to pass. The
selected tuple must remain `Unknown` in the sparse formulation. Use its sparse root snapshot and
static build certificates as the sole source of the partition domain.

Select the lowest-index network with exactly one possible internal supply option and one external
demand selector whose root key count equals the network's possible demand-option count. This is a
data-independent diagnostic rule. Record the selected network and terminal IDs and fail closed when
selection is absent or ambiguous within that network.

For the accepted Heavy Xiranite artifact, the rule selects the enriched-Xiranite-powder belt
network's external demand terminal.

## Exact Partition

Partition the selected terminal's observed sparse root keys by the encoded outward direction:

```text
north: key mod 4 = 0
east:  key mod 4 = 1
south: key mod 4 = 2
west:  key mod 4 = 3
```

Require all four sets to be non-empty, pairwise disjoint, and to have a union exactly equal to the
selected root domain. For the accepted `16 x 16` control their cardinalities are `11 / 16 / 11 /
16`.

Each child constructs the existing selected boundary-key variable directly from the complete key
set for that side. This sparse domain is root-effective; it does not rely on Pumpkin's positive
table propagator to remove absent values. Routing-option literals for keys outside that side are
not constructed. This is an exact partition of the parent root domain. It fixes no boundary cell,
route cell, item, flow, topology, or logistics component beyond the parent fixture. The four
children together preserve every legal parent solution.

## Execution

Run the four children sequentially in north/east/south/west order. Each child has:

1. one uninstrumented authoritative feasibility-only solve with a 5,000 ms budget;
2. one separate root-observation solve with a 5,000 ms budget.

Search timing and counters come only from the authoritative solve. Observation evidence may retain
a witness or proof, but cannot supply authoritative runtime claims. Construction certificates are
instrumented and descriptive only.

## Required Assertions

Interpretation is blocked unless:

- the parent A/B report is unblocked and its selected sparse case remains `Unknown`;
- one target terminal and network are selected by the stated semantic rule;
- the four side key sets are non-empty, pairwise disjoint, and exactly cover the selected sparse
  root domain;
- every child is built from one common prepared input, fixed dimensions, four fixed placements and
  rotations, and fifteen fixed facility ports;
- exactly one external-terminal build certificate records the posted side restriction;
- that certificate identifies the selected terminal and its recorded restriction equals the
  child's expected key set;
- the selected terminal's observed child root domain is a subset of its side key set when a root
  snapshot is available;
- all other external terminal IDs, domains, tables, options, and network IDs remain identical
  across children;
- the four parent-fixed facility placements/rotations and fifteen facility ports remain fixed;
- model-metric differences are confined to the selected boundary key's exact side cardinality and
  its downstream routing-option encoding; the report records each child model scale instead of
  claiming identical constraint graphs. The controlled model contract subtracts the selected key
  count from exactly `boundary_terminal_variables` and `crossing_constraints`: each retained key
  creates one boundary equality literal and one terminal-option crossing guard;
- fixed-state assertions pass and no invalid witness or witness/proof conflict appears.

A child proven infeasible before the root observer runs still requires its static restriction
certificate; no fabricated root domain is required.

## Evidence Aggregation

- Any validated child witness proves case 6 and its parent feasible.
- All four children proven infeasible prove case 6 infeasible; case 12 becomes the next remaining
  parent leaf.
- Any witness/proof conflict or invalid witness blocks interpretation.
- Otherwise every unresolved child remains `Unknown`; no missing child is treated as infeasible.

## Report

Record JSON and self-contained HTML containing:

- parent A/B provenance and selected terminal/network rule;
- exact side key sets and coverage assertions;
- authoritative and observation outcome per child;
- construction/search/first-incumbent times;
- decisions, backtracks, conflicts, learned clauses, and solver propagations;
- model scale and the selected terminal's root values;
- root route-cell, route-arc, arm-item, flow, and first-decision summaries;
- aggregate proof/witness/Unknown status and the next exact target.

## Stop Condition

Commit and report this four-way diagnostic before changing dimensions or routing semantics.

- If one side resolves, refine unresolved sides by exact boundary-cell equality.
- If all four remain `Unknown` with little root reduction, run the complete fixed-height widths
  `13 x 16` through `16 x 16` using the sparse formulation.
- If both experiments remain unresolved, map the first unregistered Boolean to its semantic source
  before any false/true partition. Never partition a raw runtime domain ID.
