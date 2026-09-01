# Phase 3 Endpoint Source-Only Exact Control

## Status

Accepted diagnostic contract for separating source-continuation propagation from demand-case
enumeration in the controlled Phase 3 leaf.

## Question

Does fixing only the canonical source continuation `48 -> 32`, while leaving every demand
continuation free, make the internal liquid-xiranite-poly case root-infeasible?

## Parent

The parent is the accepted internal-pipe endpoint-continuation report. It provides:

- the exact key-24 `16 x 16` fixed placement/rotation/port leaf;
- one fixed positive source and one fixed positive demand for
  `network:pipe:item-liquid-xiranite-poly`;
- the complete ordered source-continuation candidates;
- the complete source-by-demand exact portfolio used to prove the whole `48 -> 32` source region
  infeasible.

## Exact Control

For each ordered source candidate `s_i`, post only:

```text
flow(s_i) >= 1
flow(s_j) = 0 for every j < i
```

Do not restrict any demand-continuation arc. Later source arcs remain free. Every placement, port,
boundary terminal, route-interior, item, flow, topology, bridge, component, and objective decision
retains the parent freedom.

The source cases are pairwise disjoint and exhaustive because the fixed positive source terminal,
non-negative flow, terminal-cell item semantics, bridge exclusion, and flow conservation require at
least one positive outgoing source arc.

## Controlled Difference

Every child must keep the parent variable count and placement-routing incidence count unchanged.
It may add exactly:

```text
1 + number of earlier source candidates
```

unary research-fixation constraints and incidences. Authoritative and observation models must be
structurally identical within each case.

## Outputs

- one source-only authoritative and observation result per canonical source candidate;
- root snapshots or explicit root-infeasible sentinels;
- boundary and source-continuation build certificates;
- facility/port fixation audits;
- model scale and solver search counters;
- comparison with the parent source-region aggregate evidence;
- machine-readable JSON and self-contained HTML.

## Interpretation

- If source case `48 -> 32` is root-proven infeasible, demand fixation was unnecessary to expose
  the contradiction. The source-side dead-end/connectivity hypothesis becomes stronger, but a
  general rule still requires reusable proof premises.
- If it remains `Unknown`, the three exhaustive demand children were necessary for proof discovery.
  The missing propagation opportunity lies across the unresolved demand disjunction rather than in
  the source fixation alone.
- `Unknown` is never interpreted as feasible or infeasible.

## Failure Modes

- parent report blocked, invalid, or lacking an exact source cover;
- source candidates empty, duplicated, or inconsistent with the parent root census;
- any explicit demand fixation appears in a source-only build certificate or controlled model
  delta; root demand-domain narrowing caused by exact propagation is allowed and reported;
- nonselected external boundary domains differ from the parent legal domains;
- facility placement, rotation, or port fixation differs;
- child model delta exceeds the canonical source-only unary restrictions;
- authoritative and observation evidence conflicts;
- invalid witness or missing root snapshot.
