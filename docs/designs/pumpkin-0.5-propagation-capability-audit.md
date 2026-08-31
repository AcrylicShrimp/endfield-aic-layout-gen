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
| `maximum` | Yes | No | Bounds-consistent | Bounds | Scans all arguments; support reasoning is bounds-only |
| Positive `table` | Exact on supported values after row propagation | Yes | Strong value support in both directions | SAT/unit propagation | One hidden Boolean per row plus row-to-value and value-to-support clauses |

The checked repository tests are in
`crates/aic-data/src/layouts/integrated/exact/propagation_capability.rs`.

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
