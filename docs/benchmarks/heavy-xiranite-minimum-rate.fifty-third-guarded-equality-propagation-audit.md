# Guarded Equality Propagation Audit

## Question

The endpoint experiment showed that semantically exact constraints can fail to propagate interior
domain holes. This audit checks whether the same Pumpkin 0.5 behavior appears in the item rules used
throughout the shared transport grid.

The important rules are:

```text
terminal option selected -> arm item = required item
route arc selected -> source arm item = target arm item
bridge selected -> opposite arm items are equal
```

## Micro-test result

Three source-backed regression tests were added:

| Probe | Semantically required result | Pumpkin 0.5 result |
|---|---|---|
| `selected -> item=1`, then remove interior value `1` from `{0,1,2}` | `selected=false` | Guard remains unknown |
| `not selected or item=1`, then remove interior value `1` | `selected=false` | Guard becomes false |
| `selected -> left=right`, with `left={0,2}` and `right={1,3}` | `selected=false` | Guard remains unknown |

All three constraints remain exact on complete assignments. The difference is when an impossible
selection is discovered.

For one-term equality, Pumpkin posts two reified linear inequalities. Each inequality is still
individually satisfiable when the required value is an interior hole, so neither can reject the
guard. For binary equality, the active propagator is domain-consistent, but the generic reified
wrapper asks its separate inconsistency detector whether the guard can be rejected. That detector
compares only the two domains' bounds.

## Phase 3 exposure

The existing 16x16 Phase 3 model report lets the affected constraints be counted exactly.

### Guarded single-item rules

The model has 10,120 facility endpoint-key Boolean literals and 5,440 boundary-terminal-key Boolean
literals. Every one is connected to its material through one guarded single-item equality:

```text
10,120 + 5,440 = 15,560 weak guard checks
```

### Guarded binary-item rules

Every directed route arc has one guarded equality between the item on its two incident arms. Every
bridge has two guarded opposite-axis item equalities:

```text
1,920 route arcs + (512 bridge cells * 2 axes) = 2,944 weak guard checks
```

When a guard becomes true, these constraints propagate correctly. Before that decision, however,
an option whose required material has lost all domain support can remain branchable. This creates a
second hole-propagation cliff independent of the placement-port Element chain.

## Exact first action

The guarded single-value rule has a simple exact reformulation:

```text
not selected OR item == required_item
```

Pumpkin predicate literals channel `item == required_item` exactly, so unit propagation rejects the
selection immediately when that item value disappears. This replacement changes neither the legal
solution set nor the solver's placement, port, routing, item, or flow decisions.

The integrated experiment must compare:

- current two reified linear inequalities;
- the predicate clause;
- model build time and RSS;
- visible and hidden variable/clause changes;
- root pruning of terminal options;
- decisions, backtracks, conflicts, learned clauses, solver propagations, and five-second outcome;
  and
- independent witness and objective validation on a feasible case.

The guarded binary equality requires a separate experiment. A complete guard-rejection channel must
detect whether the two item domains have any common value. Replacing it with a large tuple table
without measuring hidden row literals would repeat the endpoint-table mistake.

## Verification

```text
cargo fmt --all
cargo test -p aic-data propagation_capability -- --nocapture
cargo check --workspace
cargo test --workspace
git diff --check
```
