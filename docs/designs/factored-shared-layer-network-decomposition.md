# Factored Shared-Layer Network Decomposition

## Status

Accepted diagnostic experiment. It does not change solver semantics or select a production
strategy.

## Question

After sharing physical transport layers and factoring placement from port choice, at what network
composition does Heavy Xiranite cumulative SCC phase zero first fail to produce an incumbent within
the fixed research budget?

## Controlled Formulation

Every case uses `joint-shared-transport-layer-factored-endpoints-v1` with unchanged:

- facility placement and rotation search;
- independent exact port choice and element-derived terminal geometry;
- item-labelled shared belt and pipe layers;
- flow, capacity, branch, bridge, collision, and geometry constraints;
- lexicographic objectives; and
- Pumpkin search configuration.

No decision is fixed and no constraint family is removed in this checkpoint.

## Case Construction

The phase-zero input currently contains three commodity networks. The matrix rebuilds the exact
model from logical edges for:

1. each single network;
2. each two-network pair; and
3. the complete three-network phase.

Rebuilding from logical edges is required. Merely disabling route arcs would retain excluded
network ports, terminal geometry, item domains, and auxiliary variables and would not measure a
clean composition boundary.

Each case receives an independent equal release-mode search budget. Timeout means `unknown`, not
infeasible.

## Outputs

The report records for every case:

- selected network indices and stable network IDs;
- status, first incumbent, objective, proof, and validation;
- variable, domain-volume, constraint, term, and incidence counts;
- placement-routing coupling; and
- model-construction and search time.

The CLI writes one matrix JSON plus one self-contained HTML result for every case, including cases
without geometry.

## Decision Boundary

This checkpoint stops after locating the first network-composition cliff and identifying its
largest remaining model families. Any subsequent constraint-family removal or fixed decision is a
separate diagnostic ablation and requires an explicit next research decision.
