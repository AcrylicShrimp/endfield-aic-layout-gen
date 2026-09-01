# Phase 3 Material Junction Continuation

## Status

Accepted experiment contract for the exact diagnostic following the row-4 material separator cut.

## Question

The inherited Phase 3 leaf forces selected pipe material `item-liquid-xiranite-poly` to enter cell 80
through `64 -> 80`, but it still finds no first witness in five seconds. Does partitioning the first
material-carrying continuation leaving cell 80 break the next search cliff?

## Parent Evidence

The experiment is valid only for the accepted fixed Phase 3 fixture:

- exact dimensions `16 x 16`;
- four facilities and all fifteen fixed facility terminals retained;
- external boundary key 24 retained;
- selected network `network:pipe:item-liquid-xiranite-poly`;
- fixed selected source continuation `48 -> 64`;
- row-4 material separator case 0, `64 -> 80`, retained;
- demand terminal at cell 113 and every demand continuation unrestricted;
- the preceding row-4 portfolio unblocked, exact, and compatible with its control;
- case 0 is the sole non-infeasible row-4 child and remains `Unknown`.

The parent control root already implies the row-4 case-0 predicate. The new model nevertheless
posts the case-0 restriction explicitly so every child has a direct, auditable inherited leaf
contract.

## Semantic Argument

Cell 80 is the westmost cell of row 5. For the selected material network:

1. positive selected-material flow enters through `64 -> 80`;
2. cell 80 is neither the selected source nor the selected demand terminal;
3. no west neighbor exists;
4. the reverse arc `80 -> 64` cannot be selected together with `64 -> 80` under the base opposing
   directed-arm constraint;
5. base flow conservation and item continuity therefore require positive selected-material flow on
   at least one of `80 -> 81` or `80 -> 96`.

For a candidate directed arc `e`, define:

```text
Q(e) <-> selected(e) = 1 AND from_item(e) = selected_item_code
```

The candidate order is east then south. The exact canonical children are:

```text
E: Q(80 -> 81)
S: not Q(80 -> 81) AND Q(80 -> 96)
```

The E child intentionally permits both continuations. The S child excludes only selected-material
use of the east arc; it does not exclude another material from using that arc. Thus the candidate
set is non-empty and the two formulas are pairwise disjoint and exhaustive without assuming a
unique route, forbidding a splitter, or fixing downstream topology. Either individual child may be
empty and proven infeasible.

## Solver Encoding

Add a reusable explicit material-continuation restriction to the shared-layer exact model. It must:

- resolve the requested network and layer-local item code from the model;
- validate the incoming arc and both continuation arcs against the actual shared layer;
- validate declared route-selection, flow, and arm-item variable families and domains;
- require every candidate to originate at cell 80 and reject duplicates;
- for child `i`, post two unary constraints for `Q(i)`;
- for each preceding `j`, post the native binary predicate clause
  `selected(j) = 0 OR from_item(j) != selected_item_code`;
- create no candidate-selector variable, table, row-selector, route template, or heuristic domain
  restriction;
- emit raw build certificates containing the actual variable IDs, names, families, declared bounds,
  and posted constraint counts.

The raw certificate and interpretation gate must also verify the theorem premises against the
actual built model:

- the selected network has exactly the fixed source and demand terminals, neither at cell 80;
- cell 80 is on the west boundary of the actual `16 x 16` grid;
- the actual outgoing grid arcs from cell 80 are exactly north, east, and south, with no west arc;
- the inherited `64 -> 80` arc is explicitly fixed selected with item code 5;
- the base opposing-arm constraint and the non-bridge and bridge flow/item-continuity formulations
  used to prove the cover are present in the unchanged semantic model identity.

The inherited row-4 restriction and the new junction restriction are independent semantic audit
objects. Both must be visible in authoritative and root-observation certificates.

## Root Observation

The control carries the same junction audit/probe object with `selected_case_index = None`. It emits
the same raw candidate certificate and root observations but posts zero junction variables,
constraints, or incidences.

Capture, for the incoming arc and both candidates:

- `case_index`, using `null` for the inherited incoming arc and `0..N` only for actual
  continuation candidates;

- selected domain;
- flow domain;
- from-arm item domain;
- selected item code;
- candidate index and direction.

The control must post no junction restriction. Each child must show exactly the intended selected
predicate effects at root unless the child is root-infeasible. The preceding exclusion in the S
child is relational: it need not fix either the route-selected scalar or item scalar by itself. Its
presence is certified by the raw native-clause certificate and exact family counts, not by demanding
an unsupported scalar-domain change. Hidden solver-domain counts, variable-family summaries,
layer/network summaries, and the first delegated decision remain part of the comparison.

The expected controlled deltas are fail-closed:

| Comparison | Variables | Constraints | Incidences | Placement-routing incidences |
| --- | ---: | ---: | ---: | ---: |
| row-4 case 0 to junction control | +0 | +0 | +0 | unchanged |
| control to E child | +0 | +2 | +2 | unchanged |
| control to S child | +0 | +3 | +4 | unchanged |

The two selected constraints and one preceding clause must belong to a dedicated
`material-junction` constraint family. Authoritative and observation models must have identical
formulation identity, complete static model scale, and model-complexity census within each case.
Captured roots must also have identical solver, registered, and unregistered domain coverage.
Root-infeasible children may use the static zero-auxiliary certificate and must report dynamic
hidden-domain equality as unavailable rather than claiming it was observed.

## Execution Matrix

Run one unrestricted junction control, then the two children. Run an independent root-observation
copy for each. Use a five-second search budget for every authoritative and observation solve. Child
solves are independent and may run concurrently.

Record:

- model construction and search time;
- first incumbent and outcome;
- branch decisions, backtracks, conflicts, learned clauses, and solver propagations;
- variables, constraints, incidences, and placement-routing incidences;
- authoritative/observation agreement;
- root domains for the inherited incoming arc and both continuations;
- exact-cover, fixture, certificate, model-identity, and parent-child evidence gates.

## Artifact Contract

The report uses a new explicit schema version and writes:

- machine-readable summary JSON;
- a self-contained summary HTML page;
- authoritative and observation wireframes for the control and both children, including failure
  evidence when no geometry exists;
- raw authoritative and observation row-4 and junction certificates;
- execution order and per-wave/total wall times;
- an explicit `interpretation_blocked` field and every constituent gate.

## Interpretation Gates

Interpretation is blocked if any of the following occurs:

- the accepted fixture or parent proof chain differs;
- the row-4 case-0 restriction is not retained exactly;
- the two candidates are missing, duplicated, reversed, or not east/south from cell 80;
- authoritative and observation certificates differ;
- raw variable family or declared-domain checks fail;
- the control posts a junction restriction;
- a child posts any restriction beyond its two selected unaries and canonical preceding clauses;
- any expected variable, constraint, incidence, constraint-family, hidden-domain, or
  placement-routing delta differs;
- authoritative and observation outcomes conflict;
- a child witness fails validation;
- a child proof contradicts a compatible parent witness;
- the two children do not form a complete disjoint cover.

## Outcome Interpretation

- A validated witness proves the inherited leaf feasible and makes the continuation split a
  first-feasible breaker.
- If E is proven infeasible, selected-material east use is impossible and south is required; continue
  from S if it remains `Unknown`.
- If S is proven infeasible, east is required but south may still also be used because E intentionally
  includes splitters; continue from E if it remains `Unknown`.
- Two `Unknown` children leave the immediate direction unresolved. A row-5 material separator may
  then refine the lowest-index unresolved child as a child-local representative diagnostic, but it
  does not cover the S branch or the complete inherited leaf unless both continuation branches are
  eventually covered.
- Two proven-infeasible children prove the inherited row-4 leaf infeasible.
- If this split and the subsequent row-5 split are both root-redundant or reproduce the control root
  summaries and first delegated decision without witness/proof progress, stop route-local
  refinement and design a cross-network or shared-topology partition. Timeout counter differences
  remain descriptive and are not a semantic ranking.

## Non-Goals

This experiment does not:

- choose a route outside the solver;
- require the route to be straight, short, acyclic, or unbranched;
- forbid shared trunks, splitters, convergers, or cycles;
- restrict placement, ports, demand continuation, flow magnitude, bridges, other materials, or
  downstream cells;
- claim that `16 x 16` is a production optimum or game limit;
- change the production solver architecture.
