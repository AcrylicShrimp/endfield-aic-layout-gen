# Phase 3 Exact Material Separator-Cut Experiment

## Purpose

The accepted source-only control proves that source continuation `48 -> 32` is root-infeasible in
the inherited Heavy Xiranite Phase 3 key-24 leaf. The complementary source continuation `48 -> 64`
remains `Unknown` after five seconds while all three demand continuations remain free.

This experiment partitions one mandatory interior crossing of the selected material without
choosing a complete path, a demand continuation, a shortest route, a corridor, or any facility
state. It tests whether the unresolved route-interior disjunction is the next first-feasible cliff.

## Parent Contract

Reproduce the accepted source-only report and fail closed unless:

- every parent interpretation gate passes;
- the selected network is `network:pipe:item-liquid-xiranite-poly`;
- it has exactly one fixed positive source terminal and one fixed positive demand terminal;
- the canonical source partition contains exactly the two accepted candidates;
- source case `48 -> 32` is proven root-infeasible;
- source case `48 -> 64` is unresolved and retains no explicit demand-continuation restriction;
- source cell 48, selected continuation destination cell 64, and demand cell 113 are unchanged;
- all four facility placements/rotations and all fifteen facility ports retain the inherited fixed
  contract;
- the fixed grid is 16 by 16 and the selected separator row is supplied explicitly by the caller.

The fixed grid is a controlled diagnostic fixture, not a production dimension or game limit.

## Separator

The caller supplies `separator_after_row = r`. The separator contains every south-directed grid arc

```text
(x, r) -> (x, r + 1), for x in [0, width)
```

in canonical increasing-`x` order.

Fail closed unless:

- `0 <= r < height - 1`;
- the source terminal and selected source continuation destination are on or above row `r`;
- the fixed positive demand terminal is below row `r`;
- every canonical crossing is a modeled directed arc on the selected transport layer;
- the selected material has one fixed positive source and one fixed positive demand and no other
  terminal of that material network can inject or consume flow across the cut.

For the accepted fixture, `r = 4`: source cell 48 is on row 3, selected continuation destination
cell 64 is on row 4, and demand cell 113 is on row 7.

## Exact Material-Carrying Predicate

For each south-directed cut arc `e`, define the existing-state predicate

```text
Q_e := selected_e = 1 AND from_arm_item_e = selected_item_code.
```

No `Q_e` variable, table, row selector, or other auxiliary solver state is introduced. The base
model already proves:

```text
selected_e = 1  <->  flow_e >= 1
selected_e = 1  ->  from_arm_item_e = to_arm_item_e.
```

Therefore `Q_e` is exactly “arc `e` carries positive flow of the selected shared commodity.” The
predicate is expressed directly through the existing route-selected and arm-item domains. This
avoids Pumpkin positive-table row selectors, which are already a measured search burden in this
repository.

## Exact Partition

The unpartitioned control is the same surviving source-only case with no material-crossing
restriction. It introduces no variables or constraints. It is executed through the separator-audit
path only to capture the raw cut certificate and typed per-crossing root domains.

Canonical child `i` posts only:

```text
selected_i = 1
from_arm_item_i = selected_item_code

selected_j = 0 OR from_arm_item_j != selected_item_code, for every j < i.
```

The selected crossing uses two unary exact constraints. Each earlier crossing exclusion is one
native binary predicate clause; it must not force the earlier physical arc off because that arc may
still carry another item. No later crossing is constrained.

These child restrictions are exactly `Q_i` and `not Q_j` for every `j < i`. The children are
pairwise disjoint. They are exhaustive because flow conservation and the fixed positive
source/demand on opposite sides imply positive net southward flow across the separator, hence at
least one south-directed cut arc carries the selected item with positive flow.

Northward recrossings, cycles, branches, convergers, splitters, bridges, shared pipe cells, all
three demand continuations, and every route state on either side of the cut remain solver decisions.

## Solver Input

The research entry point receives:

- the same catalogs, workload, placement request, Phase 3 target, inherited fixation, endpoint
  network, and budgets as the accepted source-only control;
- `separator_after_row`;
- one child authoritative budget and one child observation budget;
- worker count.

Run one no-restriction control authoritative solve and one separate observation solve first. Then
run the child authoritative wave followed by the separate child observation wave. Use the fixed
worker count only within each child wave. The active five-second default applies independently to
every solve. Performance timings and counters come only from authoritative solves; observation
solves contribute root/proof evidence.

## Solver Output

The machine-readable report records:

- the complete source-only parent report;
- separator row, source/destination rows, width, and every canonical crossing arc;
- selected network, transport, item, network index, and layer-local item code;
- raw authoritative and observation build certificates for control and every child, including
  transport/layer, network/index/item/code, ordered crossing arcs, semantic route/flow/item domain
  names, declared domains, selected case, and earlier exclusions;
- unpartitioned control result and typed root route-selected/flow/from-item domains for every
  crossing;
- every canonical child restriction, result, typed crossing root domains, and search counters;
- exact model variables, constraints, incidences, and family counts;
- control-versus-base and child-versus-control model deltas;
- authoritative/observation model and certificate identity;
- exact-cover, fixation, evidence-compatibility, invalid-witness, and interpretation gates;
- control authoritative/observation wall times, child authoritative/observation wave times, total
  reconstruction time, and explicit execution order;
- JSON, self-contained summary HTML, and per-run wireframes for success or failure.

## Required Audit Gates

Interpretation is blocked unless:

- all parent and separator proof obligations pass;
- the candidate list is non-empty, unique, complete, and canonically ordered;
- every crossing uses the selected pipe layer, exact south-directed arc, declared flow domain,
  correct from-side arm-item domain, and selected layer-local item code;
- the control adds zero variables, constraints, incidences, or hidden solver domains relative to
  the source-only base;
- child `i` adds zero variables, exactly two unary constraints plus `i` binary predicate clauses,
  and exactly `2 + 2*i` recorded incidences relative to the control;
- a dedicated material-separator constraint family distinguishes the unary selections and binary
  exclusions from unrelated research fixations;
- the recorder stores every binary predicate clause without creating an auxiliary literal;
- no child explicitly fixes a demand continuation or any route state outside its selected crossing
  and canonical earlier-crossing exclusions;
- authoritative and observation certificates, formulation identifiers, exact model metrics, and
  complexity metrics match within every case;
- root snapshots confirm the selected crossing is active with positive flow and singleton selected
  item when root capture is available;
- earlier exclusions are certified as relational clauses and are not incorrectly required to fix
  either scalar domain at root;
- a root-infeasible sentinel is accepted only with a proven-infeasible outcome and matching raw
  build/restriction certificates; it never claims dynamic root-domain observation;
- all inherited facility, port, boundary, and source-continuation fixation contracts pass;
- no invalid witness or witness/proof contradiction occurs.

## Outcome Interpretation

- Control outcome differs logically from the accepted source-only parent: block only on a real
  witness/proof contradiction; timeout differences remain cutoff-censored.
- Control `Unknown`, child witness: the exact separator partition is material to first-feasible
  search.
- Mixed proven-infeasible and `Unknown` children: continue inside the lowest unresolved child with
  a second exact separator or another measured interior state.
- Every child proven infeasible: the complete `48 -> 64` source region is infeasible; combined with
  the accepted `48 -> 32` proof, the inherited key-24 leaf is infeasible.
- Every child `Unknown`: one selected network's first interior cut is not the current practical
  breaker; return to cross-network, external-terminal, or topology coupling evidence.
- Invalid witness, certificate mismatch, incomplete cover, or proof conflict: block interpretation
  and repair the experiment.

Timeout counter differences are descriptive and never rank cases as faster or prove feasibility.

## Non-Goals

This slice does not:

- change production solver semantics or orchestration;
- fix a route, path length, corridor, demand continuation, bridge, or facility decision;
- assume an acyclic route;
- promote a leaf-local inference to a general propagator;
- claim that 16 by 16 is an optimal or required blueprint size;
- claim optimality from a first feasible witness.
