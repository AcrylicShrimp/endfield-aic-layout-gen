# Phase 3 Guarded Infeasible-Core Replay

## Status

Proposed controlled exact experiment. This does not replace the authoritative joint solver and
does not authorize a production master/subproblem cutover.

## Question

The accepted Phase 3 row-5 experiment proves one nontrivial route branch infeasible:

```text
junction child E and Q(80 -> 96)
```

Can that proof be reduced to a materially smaller conjunction of native solver predicates and then
replayed as one sound guarded nogood in the otherwise unrestricted fixed-ceiling joint model?

This experiment tests the feedback mechanism needed by a future exact progressive decomposition.
It does not test the full decomposition architecture.

## Controlled Fixture

- production graph: Heavy Xiranite minimum-rate cumulative Phase 3;
- hard search ceiling: `16 x 16`;
- controlled used dimensions in the accepted leaf: `16 x 16`;
- solver stack: sparse legal external endpoints, possible-graph connectivity, watched-demand unique
  support, local continuation, and guarded positive-item intersection;
- per authoritative deletion attempt: 5 seconds;
- no optimization objective during core extraction;
- prior exact solution may be supplied only as a non-binding solver hint.

The `16 x 16` values are fixture inputs, not project or game invariants.

## Native Atom Contract

Every core atom denotes one semantic predicate over an existing Pumpkin integer domain. Every
assumption, including the external-boundary-key atom, is posted into the same unrestricted sparse
base model as a native predicate. Boundary routing options are materialized from the full legal
sparse key domain in extraction, deletion attempts, and replay. The accepted singleton-domain
predecessor is evidence for the initial semantic vector only; its proof is not transferred into this
experiment. The replay clause contains the logical complement of every retained atom:

```text
not(A0 and A1 and ... and An)
==
not A0 or not A1 or ... or not An
```

The experiment introduces no core-selector variables. All assumption, final-proof, and replay
models use identical unrestricted declared domains before research predicates are posted.

Predicate complements are relation-specific and are derived by the replay builder:

```text
x = value   -> x != value
x != value  -> x = value
x >= value  -> x < value
x <= value  -> x > value
```

For the current non-negative flow domain, the complement of `flow >= 1` is `flow <= 0`. The clause
certificate records the original relation, value, derived complement relation, and complement
value. A generic disequality is not a valid replacement for an inequality complement.

Supported atom kinds are:

```text
used-width(value)
used-height(value)
placement(instance, x, y, rotation)
facility-port(terminal, port)
external-boundary-key(terminal, key)
material-arc-selected(network, from, to)
material-arc-item(network, from, to, item)
material-arc-flow-at-least(network, from, to, minimum)
material-arc-flow-equals(network, from, to, value)
```

Resolution is fail-closed. Every atom certificate records its stable semantic ID, native domain ID,
variable family, variable name, declared domain, predicate relation, and expected value. The
boundary certificate additionally records the unrestricted legal values. Missing, duplicate,
ambiguous, wrong-family, or out-of-domain atoms make the experiment invalid before search.

Identity comparison has two scopes. Non-boundary predecessor atoms are compared by stable semantic
ID, variable name and family, predicate relation, declared bounds, and expected value. The
predecessor boundary-key atom instead compares stable terminal ID, variable name and family,
predicate `key = 24`, expected value, and recorded legal-key/routing-option semantics; its singleton
declared domain, bounds, and numeric domain ID are explicitly allowed to differ from the normalized
unrestricted model. All normalized experiment builds share the unrestricted base and must have
identical full declared domains and native numeric domain IDs. An allowed predecessor boundary
domain or numeric-ID difference alone is not a premise mismatch.

### Material predicate expansion

For this formulation:

```text
Q(from -> to, item)
==
route_selected(from -> to) = 1
and
from_arm_item(from -> to) = item_code
```

Each separator or junction `Q` therefore contributes two native equality atoms. The earlier
endpoint-continuation experiment used native flow predicates instead: its selected arc contributes
`flow >= 1`, and its preceding arc contributes `flow = 0`. The core vector must preserve that
actual encoding rather than replacing it with an inferred route/item encoding.

## Initial Case-0 Atom Vector

The initial vector has 30 atoms in this stable order.

### Used dimensions: 2

```text
used width  = 16
used height = 16
```

### Facility placements: 4

```text
mix-pool liquid-xiranite occurrence       @ (1, 0),  rotation 180
xiranite-oven powder occurrence           @ (1, 11), rotation 90
target xiranite oven                       @ (2, 5),  rotation 0
second xiranite-powder input oven          @ (8, 5),  rotation 0
```

The exact report stores full stable instance IDs; the names above are explanatory labels only.

### Facility terminal ports: 15

The vector contains the eleven accepted inherited/tuple assignments and the four accepted boundary
facility assignments. All fifteen terminal IDs are distinct. The generated report must list every
stable terminal and port ID.

### External boundary endpoint: 1

```text
terminal wiring-edge:7565...:demand has boundary key 24
```

The generated report stores the complete stable terminal ID.

### Route state: 8

The source-continuation restriction contributes:

```text
flow(48 -> 64) >= 1
flow(48 -> 32) = 0
```

Two native atoms are emitted for each later selected-material predicate:

```text
Q(64 -> 80)
Q(80 -> 81)
Q(80 -> 96)
```

## Initial-Model Equivalence Gate

Before shrinking, the full 30-atom model must:

1. be proven infeasible within the authoritative budget;
2. match the accepted workload, Phase 3 cumulative graph, `16 x 16` ceiling, item-code table,
   network table, and solver/formulation fingerprints;
3. resolve every atom to the expected native variable family and declared domain;
4. compare the ordered, distinct fifteen-entry terminal-to-port vector, four placement-choice
   identities, complete external terminal ID and key, and all eight native route-variable semantic
   identities against the authoritative predecessor data, without requiring predecessor numeric
   domain IDs to match;
5. reproduce all 30 intended native predicates and no others;
6. match the stable descriptors and declared domains of every non-research variable, plus the
   complete base constraint-family and incidence vector, between the initial, deletion,
   final-proof, control, and replay builders;
7. confirm `flow(48 -> 64) >= 1` and `flow(48 -> 32) = 0` at root;
8. contain no placement, port, boundary, or route restriction outside the atom vector;
9. have no invalid-witness, evidence conflict, certificate mismatch, or model-family drift.

The accepted singleton-domain predecessor is allowed to have a different structure, but it cannot
authorize a replay cut. The new unrestricted-domain full-core solve must independently prove the
same conjunction infeasible. Within this experiment, all models share one normalized base model.
The only allowed deltas are the explicitly listed unit predicates removed by a deletion attempt,
the retained unit predicates in a final-core proof, or the one certified replay clause. Any other
variable, domain, constraint-family, or incidence delta blocks the experiment.

Failure of any gate stops the experiment. A timeout is not an equivalence proof.

## Sequential Proof-Only Shrinking

Start with the complete ordered atom vector `C`.

For each atom `a` in stable order:

1. solve a fresh exact model with assumptions `C - {a}`;
2. if and only if the authoritative result is **ProvenInfeasible**, permanently set
   `C := C - {a}`;
3. if the result is `Unknown` or validated feasible, retain `a`;
4. if the result is invalid, conflicting, certificate-mismatched, or model-drifted, stop the entire
   experiment as blocked;
5. otherwise continue with the updated `C`.

This produces a sound, order-dependent **proof-shrunk core**. It is not claimed to be
deletion-minimal, cardinality-minimum, or unique. An atom retained after `Unknown` is budget-censored,
not proven necessary. Search counters, runtime, or a promising partial trace never justify removing
an atom.

Every attempt records:

- attempted atom and prior core size;
- exact retained atom IDs;
- outcome and termination reason;
- build/search/wall time;
- branch decisions, backtracks, conflicts, learned clauses, and solver propagations;
- model variables, constraints, incidences, and family deltas;
- raw atom-resolution certificates;
- whether the atom was removed and the proof that authorized removal.

Execution is staged so observer evidence cannot influence proof decisions:

1. initial full-core authoritative solve;
2. separate initial full-core observation solve;
3. the complete sequential authoritative deletion wave, with no observer feedback;
4. separate final-core authoritative and observation solves;
5. replay control and cut authoritative wave;
6. separate replay control and cut observation wave;
7. exact high-level breadth observation.

Each stage records its independent budget, execution order, model fingerprint, and certificate
agreement. The final core must independently be **ProvenInfeasible** authoritatively, and its root
observer must not conflict with that proof. Every final atom certificate must be identical across
the two builds. Observer counters never authorize deletion or support a performance conclusion.

## Guarded Replay A/B

The replay target is the ordinary Phase 3 joint feasibility model with a `16 x 16` search ceiling:

- placement, rotation, ports, external boundary endpoints, and all routing are free;
- no row separator, junction, endpoint continuation, or accepted-leaf fixation is posted;
- actual used width and height remain free; dimension equalities apply only when their atoms remain
  inside the guarded clause;
- the prior solution is at most a non-binding hint;
- the control and replay models differ only by the guarded core clause and its certificate.

The control receives no cut. The replay receives exactly one clause and zero new variables:

```text
not(all final-core atoms)
```

Its arity must equal the final-core size, and its ordered literals must exactly equal the certified
native complements of the retained predicates. The normalized model delta must contain this one
clause, its expected incidences, and nothing else. An empty final core means the unrestricted base
model itself was proven infeasible; it blocks useful-cut interpretation and is not emitted as a
normal replay clause.

The cut is sound because the final conjunction has an authoritative infeasibility proof in the
same base formulation and search ceiling. It may not be applied to another phase, ceiling,
item encoding, or solver formulation without a fresh proof.

The A/B records:

- first feasible/proof/`Unknown` outcome at 5 seconds;
- first-incumbent time and objective when available;
- branch decisions, backtracks, conflicts, learned clauses, and solver propagations;
- root-domain changes;
- model and constraint-family delta;
- guarded clause arity;
- how many root placement, port, boundary, and material domains it prunes.

## Exact High-Level Breadth Portfolio

Core size alone does not show that the cut is reusable. Breadth is measured against a finite exact
portfolio already enumerated by the accepted chain:

```text
16 residual facility-port tuples
x 54 root-supported boundary keys for the selected external terminal
= 864 high-level assignments
```

All 864 assignments retain the accepted four placements, inherited eleven port assignments,
`16 x 16` used dimensions, workload, and model ceiling. The Cartesian product is constructed from
the exact port domains and legal sparse boundary-key domain, not from a heuristic sample. A control
root solve records which assignments remain live before the cut. This portfolio is exact only
inside the finite predecessor subspace defined by those four residual port domains and selected
external terminal. It is not claimed to enumerate every port tuple in the unrestricted Phase 3
model.

The breadth run has this fail-closed execution contract:

1. generate exactly 864 distinct `(port tuple, boundary key)` pairs and certify Cartesian-product
   completeness and uniqueness;
2. build every case from the unrestricted replay base with all row-separator, junction,
   continuation, and other route-leaf restrictions removed;
3. execute a fresh control root-only build and propagation for every case with no search;
4. finish that wave before executing a fresh cut root-only build and propagation for every case;
5. use one predeclared worker count per wave, record it, and forbid solver/model reuse, incumbent
   exchange, observer feedback, or overlap between the control and cut waves;
6. require each paired model to differ by exactly the one certified guarded clause and its expected
   incidences.

Breadth root outcomes are reported separately from the 5-second replay search. Any missing,
duplicate, invalid, certificate-mismatched, or base-model-drifted case blocks the complete breadth
interpretation rather than being omitted from its denominator.

For every control-root-live assignment, the report records:

- **matched**: the assignment satisfies every retained used-dimension, placement, facility-port,
  and boundary-key atom in the core's high-level projection;
- **newly root-eliminated**: the exact assignment is root-consistent in the control but reaches a
  root conflict when the one guarded clause is added;
- **route-only guarded**: the high-level projection matches, but retained route atoms are not all
  root-forced, so the cut removes only the certified route substate rather than the whole
  high-level assignment.

The candidate list, control liveness, projection match, and conflict certificate are emitted
machine-readably. A deleted atom that is implied by retained atoms does not increase breadth.

## Interpretation

The experiment is successful enough to justify a two- or three-iteration decomposition prototype
only if all of the following hold:

1. the final core is certificate-valid and has an independent authoritative infeasibility proof;
2. at least two distinct control-root-live assignments in the 864-case portfolio are matched;
3. replay additionally achieves at least one of these exact effects:
   - the authoritative 5-second outcome improves from `Unknown` to validated feasible or
     `ProvenInfeasible`;
   - at least one previously live placement, port, or boundary root-domain value is removed in the
     unrestricted replay A/B;
   - at least two control-root-live high-level assignments are newly root-eliminated;
4. no legal witness or objective quality is lost.

Classification order is fail-closed: any proof, model-identity, certificate, or breadth-interpretation
failure is **BLOCKED** first. Breadth blockers include a case count other than 864, a missing or
duplicate pair, an invalid case, wave overlap or model reuse, and any paired-model delta failure.
If no block exists, satisfying every numbered success gate above is **GO**. Every other complete
and valid result is **STOP**.

The experiment stops without escalation if:

- fewer than two control-root-live high-level assignments are matched;
- replay remains `Unknown`, removes no live high-level root-domain value, and newly root-eliminates
  fewer than two high-level assignments;
- replay only reproduces the approximately 7.5% surviving-child counter change;
- the cut cannot be expressed without conditioning on the complete accepted leaf;

A proof-shrunk core close to the full assignment and a high count of timeout-censored deletion
attempts remain advisory evidence about weak reuse. They are reported numerically but do not
override the explicit GO gates. A proof, model-identity, or certificate failure is **BLOCKED**, not
a negative architecture result.

Neither outcome changes the authoritative joint-solver contract automatically. Replacing that
architecture still requires a separate measured proposal and explicit user approval.

## Artifacts

The CLI must emit artifacts for success, proof, `Unknown`, invalid input, and blocked
interpretation:

```text
summary.json
summary.html
initial-full-core.authoritative.html
initial-full-core.observation.html
attempt-N.authoritative.html
final-core.authoritative.html
final-core.observation.html
replay-control.authoritative.html
replay-control.observation.html
replay-cut.authoritative.html
replay-cut.observation.html
```

The HTML summary must show the ordered deletion history, final core, proof/certificate gates, model
deltas, and replay A/B statistics. It must not describe `Unknown` as infeasible.
