# Pumpkin 0.5 Propagation Capability Audit

## Purpose

An exact constraint can preserve the correct complete solution set while still propagating partial
domain information too weakly for the AIC joint placement-routing model. This audit records the
actual Pumpkin 0.5 behavior on which the formulation relies. It is based on dependency source and
repository micro-tests, not only public constraint names or comments.

The first audited path is:

```text
facility placement candidate
  -> physical port geometry
  -> logical port selection
  -> endpoint geometry key
  -> route endpoint literals
```

## Capability matrix

| Constraint or channel | Bounds | Interior holes | Backward support | Wake events | Important cost |
|---|---|---|---|---|---|
| Predicate literal `L <-> X == v` | Exact | Exact for `v` | Exact both ways | SAT/domain channel | One Boolean view and defining clauses |
| Constant or variable `element` | Yes | No, unless a hole changes a bound | Removes index values only when the selected element interval is disjoint from the RHS interval | `ANY_INT`, low priority | RHS-bound explanations scan the array; notifications may wake without useful hole filtering |
| General linear `<=`, `>=`, or equality with at least three terms | Yes | No | Bounds support only | Bound changes on affine views, high priority | Reasons are proportional to arity |
| Two-term affine equality without proof logging | Exact | Exact | Exact both ways | `ANY_INT`, high priority | Pumpkin selects its binary-equality propagator, tracks removed values, and synchronises after backtracking |
| Two-term affine equality with proof logging | Yes | No | Bounds support only | Bounds | Pumpkin deliberately falls back to two linear inequalities |
| Guarded one-term equality `L -> X = v` | Exact after `L=true` | Does not reject `L` when `v` is an interior missing value | Guard falsification uses the two inequality propagators' bounds checks | Bounds plus guard | Two reified linear propagators for one semantic value implication |
| Guarded binary equality `L -> X = Y` without proof logging | Exact after `L=true` | Exact while active; does not reject `L` for disjoint hole sets whose bounds overlap | Guard falsification is bounds-disjoint only | `ANY_INT` plus guard | Active equality is strong, but the undecided-guard phase is weak |
| Predicate clause `not L or X = v` | Exact | Exact for `v` | Exact both ways through predicate literals and unit propagation | SAT/domain channel | One clause plus a cached or new equality literal |
| `maximum` | Yes | No | Bounds-consistent | Bounds | Scans all arguments; support reasoning is bounds-only |
| Positive `table` | Exact on supported values after row propagation | Yes | Strong value support in both directions | SAT/unit propagation | One hidden Boolean per row plus row-to-value and value-to-support clauses |

The checked repository tests are in
`crates/aic-data/src/layouts/integrated/exact/propagation_capability.rs`.

## Confirmed guarded-item weakness

The shared-layer routing model uses guarded equalities for item compatibility:

```text
terminal option selected -> arm item = network item
route arc selected -> source arm item = target arm item
bridge selected -> opposite arm items are equal
```

Pumpkin's generic reified wrapper propagates an undecided guard to false only when the wrapped
propagator's `detect_inconsistency` method finds a conflict. A one-term equality is implemented as
two guarded linear inequalities, so a required interior value missing from the item domain is not
detected. Reified binary equality detects only disjoint bounds while the guard is unknown, even
though the active binary-equality propagator copies every hole after the guard becomes true.

The recorded Phase 3 16x16 model contains:

- 15,560 guarded single-item equalities, derived exactly from 10,120 facility endpoint-key literals
  plus 5,440 boundary-terminal-key literals; and
- 2,944 guarded binary item equalities: 1,920 directed route arcs plus 1,024 bridge axis links.

The first class has an exact standard replacement: the predicate clause `not selected or item =
item_code`. The micro-tests confirm that this clause immediately rejects the selected option after
the required item value becomes an interior hole. Its integrated cost still needs measurement
because creating an item equality predicate may allocate a Boolean view when Pumpkin cannot reuse
an existing one.

The guarded binary weakness was subsequently measured with a passive full-domain observer on the
current three-facility Phase 2 cliff. The relevant composite transport rule admits only positive
item codes when the route arc or bridge is selected; item code zero denotes an inactive arm and is
not equality support. Across 100 independent five-second dimension cases, 93 cases observed an
unresolved guard with disjoint positive-item domains. The maximum was 443 of 2,944 relations in one
case, and 81,711 disjoint checks were seen among 319,321 unresolved-guard checks. Of 12,235
case-local relation identities, 12,115 were bridge axes. This is a frequent measured opportunity
and justifies an exact active guard-rejection experiment. See
`docs/benchmarks/heavy-xiranite-minimum-rate.fifty-fifth-guarded-item-intersection-census.md`.

The active exact rule subsequently converted three Phase 2 dimension cases from five-second
unknown results into infeasibility proofs. Across the complete 100-case Phase 2 sweep it reduced
branch decisions by 10.3%, conflicts by 18.3%, and solver propagations by 11.7%, while still finding
no feasible incumbent. Approximately 99% of its guard-rejection attempts targeted bridge guards.
The weakness is therefore operationally relevant but remains one part of the three-facility cliff.
See
`docs/benchmarks/heavy-xiranite-minimum-rate.fifty-sixth-guarded-positive-item-intersection-report.md`.

The two bridge axes were then grouped under one active relation and perimeter bridge relations that
are already logically impossible in the native exact model were omitted. At 16x16 this reduces the
active structure from 2,944 to 2,312 relations and from 2,048 to 1,920 watched item domains. Three
completed Phase 2 proofs retained identical decisions, backtracks, and conflicts while relation
checks fell by approximately 19% and search time fell by 2.8% to 6.1%. The complete Phase 2 sweep
still produced no feasible incumbent. This is a useful exact implementation reduction, not the
cliff breaker. See
`docs/benchmarks/heavy-xiranite-minimum-rate.fifty-seventh-grouped-guarded-item-intersection-report.md`.

A follow-up two-pass implementation avoided provisional reason allocation on supported relations
but rescanned disjoint relations. It preserved the same 39/61 Phase 2 result and identical search
trees in completed proof cases, while adding 27% membership checks and producing no repeatable
runtime gain. The code was reverted. This closes guarded-item implementation polishing in favor of
the next Phase 2 cliff diagnosis. See
`docs/benchmarks/heavy-xiranite-minimum-rate.fifty-eighth-two-pass-guard-reason-rejection.md`.

## Confirmed endpoint weakness

The current factored endpoint encoding creates one placement integer whose values are complete
`(x,y,rotation)` candidates. For every compatible physical port, a constant `element` maps the
placement to a packed geometry key. A second variable `element` uses the logical port choice to
select the terminal's final geometry key.

The packed key is:

```text
cell * 4 + world_direction
```

Routing consumes exact equality literals for individual keys. It therefore removes impossible
route endpoints as interior holes in the geometry-key domain. Pumpkin 0.5's Element propagator wakes
on those removals but uses only minimum and maximum bounds when filtering its index and result.
Most route deductions consequently do not return through either Element to prune the logical port
or placement candidate.

This explains the measured Phase 3 behavior:

- fixing only preceding facility placements retains a five-second unknown result;
- fixing matching preceding facility ports exposes endpoint geometry and proves the selected state
  infeasible in 150 ms; and
- fixing complete endpoint state in 500 restricted cases changes all cases from timeouts to
  94--597 ms infeasibility proofs.

These are restricted infeasible cases. They identify a propagation cliff but do not prove that a
stronger endpoint channel will solve a faithful or feasible Phase 3 model.

## Comparison rules

Every endpoint-channel experiment must use the same legal tuple relation and the same sparse
projected domains. Otherwise an encoding can appear stronger merely because it starts with fewer
ghost values.

The minimum controlled restrictions are:

1. fixed placement and port, testing complete forward propagation;
2. one interior geometry hole with a fixed port, testing geometry-to-placement propagation;
3. removal of one complete direction class, testing direction-to-rotation and port propagation;
4. removal of every geometry supporting one placement value, testing last-support filtering;
5. removal of one placement value with a fixed port, testing placement-to-geometry propagation;
6. invalid boundary tuples and duplicate geometry aliases; and
7. multiple logical terminals sharing one facility placement domain.

The positive table is the propagation oracle, not the presumed production encoding. Pumpkin creates
row-selector variables internally, and those hidden variables are not currently included in the
repository model recorder. Table comparisons must report relation rows, estimated hidden literals
and clauses, build time, post-build and peak RSS, root pruning, and search statistics.

## Reformulation gate

Do not replace the endpoint channel until a candidate:

- preserves every enumerated legal `(placement, port, geometry)` tuple;
- reaches the expected oracle fixed point on the controlled cases, or states exactly which weaker
  propagation it intentionally provides;
- preserves shared placement authority across all terminals of one facility;
- preserves port identity when multiple ports alias the same geometry;
- passes independent witness validation on feasible integrated cases;
- records explanation size and custom propagation work when applicable; and
- improves faithful integrated search without relying only on a fixed infeasible contradiction.

The likely production candidate is a sparse semantic GAC channel over the ternary endpoint
relation. A small exact rotation-port-direction channel is a cheaper experiment but cannot replace
cell-level support filtering. A full positive table is the strongest standard-constraint oracle but
may recreate the placement-times-port memory cliff.

## Formulation review rule

Constraint names are not propagation guarantees. Every solver formulation used by the joint model
must be reviewed through four separate contracts:

1. **Semantic contract:** the complete assignments accepted by the constraint.
2. **Propagation contract:** the domain changes that are detected at a root fixpoint, including
   bounds, interior holes, last-support exhaustion, and propagation in every direction.
3. **Cost contract:** authored and hidden variables, clauses, propagators, wake events, scans,
   explanation sizes, build time, and memory.
4. **Proof contract:** how every inference is explained and checked under learning and
   backtracking.

An exact semantic contract does not imply a strong propagation contract. A strong propagation
contract does not imply an acceptable cost or proof contract. New channels must therefore be
compared against a small exhaustive oracle before their integrated wall time is treated as useful
evidence.

The first controlled endpoint comparison is recorded in
`docs/benchmarks/heavy-xiranite-minimum-rate.fifty-second-endpoint-channel-propagation-probe-report.md`.
