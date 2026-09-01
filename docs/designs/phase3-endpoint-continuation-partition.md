# Phase 3 Endpoint-Continuation Exact Partition

## Status

Accepted experiment contract for the next exact Phase 3 cliff diagnosis.

## Question

Does Pumpkin fail to find the first feasible witness because it must discover the first and last
grid arcs of a mandatory material route while simultaneously deciding the route interior?

## Controlled Parent

The parent is the accepted `16 x 16` cumulative Phase 3 leaf with:

- all four facility placements and rotations fixed by the accepted reference;
- all fifteen facility ports fixed;
- the final-product external demand fixed to north boundary key 24;
- every route-interior, item, flow, component, topology, and unrelated external-terminal decision
  left to the exact solver.

The selected material network is `network:belt:item-xiranite-enr-powder`.

## Exact Partition

At root, identify the singleton supply cell and singleton demand cell of the selected network. Let
`S` be every root-live grid arc leaving the supply cell and `D` every root-live grid arc entering
the demand cell, ordered by `(from, to)`.

For each `s_i in S`, define the canonical source case:

```text
flow(s_i) >= 1
flow(s_j) = 0 for every j < i
```

Define demand cases identically over `D`. The experiment executes every source/demand case pair.
Later arcs remain free, so a route may branch, merge, cross, or contain cycles exactly as the parent
model permits.

This is an exact partition only when the preflight certificate proves:

1. both selected terminals have positive flow and singleton selected geometry;
2. their terminal-presence literals are true at root;
3. no selectable same-network demand can consume flow at the supply cell;
4. no selectable same-network supply can create flow at the demand cell;
5. source and demand cells differ;
6. `S` and `D` are non-empty and contain every root-live incident arc in the required direction;
7. parent flow conservation and non-negative flow domains make at least one positive member of
   each set mandatory.

The mandatory-flow proof also depends on the accepted shared-layer semantics: a selected terminal
forbids a bridge in its cell, and every active non-bridge arm in that cell shares the terminal's
item. The experiment audits those existing semantics; it does not infer exactness from terminal
counts alone.

If any assertion cannot be established, the experiment fails closed and executes no child solve.

## Inputs

- accepted boundary-cell partition report;
- fixed width and height, initially `16 x 16`;
- selected singleton boundary key, initially 24;
- selected material-network ID;
- authoritative and observation time limits;
- worker count.

## Outputs

- a root endpoint-continuation census;
- exact-cover and semantic-model certificates;
- one authoritative and one observation result per case pair;
- model size and solver search counters;
- machine-readable JSON and self-contained HTML artifacts.

## Invariants

- No placement, port, unrelated terminal, route-interior, component, or topology choice is added.
- A case fixes only a canonical mandatory first positive outgoing arc and last positive incoming arc.
- The union of all cases equals the parent feasible set and cases are pairwise disjoint.
- Authoritative and observation models are structurally identical within each case.
- Unknown is never interpreted as infeasible.

## Failure Modes

- parent leaf missing, invalid, feasible, or proven infeasible;
- selected network or terminal not unique;
- endpoint geometry not singleton;
- preflight mandatory-continuation proof fails;
- partition empty, overlapping, or incomplete;
- child formulation/certificate mismatch;
- invalid witness or contradictory logical evidence.

## Interpretation

- A fast child witness means the endpoint-continuation disjunction materially contributes to the
  first-feasible cliff.
- Uniform timeouts mean this cut alone is insufficient; they do not prove the cut irrelevant.
- A preflight failure exposes a semantic or formulation assumption and blocks the experiment rather
  than silently weakening it.
