# Endpoint-Channel Propagation Probe

## Question

The Phase 3 diagnostics become hundreds of times easier after facility port state is fixed. The
current endpoint relation is semantically exact, but it is implemented as two nested Element
constraints. This probe asks which exact representation can propagate partial routing information
back to facility placement and port choices.

The controlled relation has four placement values, three port values, twelve reachable packed
geometry keys, and exactly twelve legal `(placement, port, geometry)` tuples. Every encoding starts
with the same sparse projected domains. Four exact encodings are compared:

1. the current nested Element structure;
2. nested Element plus one direct implication clause per legal tuple;
3. nested Element plus explicit direction and small exact direction tables; and
4. a positive table over the complete ternary relation, used as the propagation oracle.

An exhaustive test fixes every possible complete tuple and confirms that all four encodings accept
exactly the same twelve legal tuples. The experiment therefore measures propagation strength, not
different solution semantics.

## Result

| Root restriction | Nested Element | Direct tuple clauses | Direction channel | Positive-table oracle |
|---|---|---|---|---|
| Placement and port fixed | Oracle match | Oracle match | Oracle match | Oracle match |
| One interior geometry key removed, port fixed | Misses placement and geometry pruning | Removes placement, but retains unsupported geometry | Misses cell-specific pruning | Full filtering |
| Only one world-direction class remains | Retains all four placements | Retains all four placements | Reduces placement to `{0,1}` | Reduces placement to `{0,1}` |
| Every geometry supporting placement 0 removed | Retains placement 0 | Retains placement 0 | Retains placement 0 | Removes placement 0 |
| Placement 1 removed, port fixed | Retains unsupported geometry | Retains unsupported geometry | Removes only unsupported direction classes | Keeps exactly the three supported geometries |
| Four terminals jointly eliminate every shared placement | Misses root contradiction | Misses root contradiction | Misses root contradiction | Root infeasible |

The direction channel reaches the oracle fixpoint only for the complete-forward and
direction-class probes. Direct tuple clauses can exploit a fixed port and one missing geometry to
remove a placement, but they cannot detect that every possible port support for one placement has
been exhausted. The current nested Element chain reaches the oracle fixed point only after the
decisions have already made the relation effectively singleton.

Root propagation time is measured in microseconds and is too small for performance ranking. Its
value here is to confirm that the expected propagators ran. The observed ranges were:

| Encoding | Root propagation range |
|---|---:|
| Nested Element | 1--32 us |
| Direct tuple clauses | 1--16 us |
| Direction channel | 8--39 us |
| Positive table | 5--18 us |

## Hidden cost

For one endpoint, the authored and estimated internal structures are:

| Encoding | Authored integer variables | Element constraints | Direct clauses | Table rows / hidden row literals | Estimated table clauses |
|---|---:|---:|---:|---:|---:|
| Nested Element | 6 | 4 | 0 | 0 | 0 |
| Direct tuple clauses | 6 | 4 | 12 | 0 | 0 |
| Direction channel | 7 | 4 | 0 | 24 | 89 |
| Positive table | 3 | 0 | 0 | 12 | 56 |

In the four-terminal shared-placement probe, the positive table requires 48 hidden row literals and
an estimated 224 clauses. A real facility has hundreds of placement values rather than four, so a
full table can provide the right propagation while creating a separate build and memory cliff.
Current generic model counts do not include Pumpkin's hidden table row literals; future integrated
comparisons must report them explicitly.

## Root cause

The solver-facing problem is not a missing equality. The exact relation is present, but most route
deductions appear as interior holes in the packed geometry-key domain. Pumpkin 0.5's Element
propagator wakes on those changes while filtering with domain bounds. It therefore cannot see that:

- one placement has lost its final compatible endpoint cell;
- one rotation has lost every endpoint with a required world direction; or
- several terminals sharing one facility placement have jointly removed every placement value.

Fixing placement, port, or direction by hand makes the residual route problem easy because it skips
the weak partial-domain region. This is not evidence that those decisions should be made outside the
solver. It identifies the information that an exact stronger channel must expose.

## Propagator strategy

The formulation should not rely on a generic constraint name or on complete-assignment correctness.
Each semantic rule needs four separately measured properties:

1. accepted complete assignments;
2. root-domain propagation in every direction;
3. visible and hidden build, wake, scan, explanation, and memory cost; and
4. sound learning and backtracking explanations.

The endpoint relation is likely best served by more than one narrowly scoped rule:

- a small exact `(rotation, port, world direction)` channel cheaply exposes the confirmed direction
  cliff;
- a sparse endpoint-support propagator detects cell-level and last-support exhaustion without one
  solver Boolean per complete tuple; and
- equality literals continue to expose individual routing endpoint keys exactly.

This is a candidate architecture, not yet an accepted production cutover. The sparse propagator's
reason generation, support caches, backtracking behavior, and real relation size must be measured
before it can replace the current channel.

## Next exact experiment

Scale the same comparison to the actual Phase 3 introduced facility relation while excluding the
routing grid. The new facility has hundreds of placement values and four logical terminals sharing
one placement authority. Measure independently:

- relation rows and projected domain sizes;
- authored and hidden variables and clauses;
- build wall time, post-build RSS, and peak RSS;
- root values removed in each direction;
- support scans and explanation lengths for a sparse propagator prototype; and
- whether the multi-terminal shared-placement contradiction is found at root.

Only after the channel-only scaling result should an exact candidate be inserted into the faithful
Phase 3 joint model. The integrated comparison must include both a known infeasible stress state and
a known feasible witness, plus decisions, backtracks, conflicts, learned clauses, solver
propagations, build/search time, RSS, and objective/witness validation.

## Independent review disposition

Independent reviews agreed on the Element interior-hole diagnosis and the positive table as the
strong oracle. They blocked an immediate production cutover because the current fast cases are
restricted infeasible states and because a custom generalized-arc-consistency propagator does not
yet have a measured explanation or memory contract. The controlled probe confirms that caution:
the cheap direction channel solves one real propagation class but not the cell-level or shared
last-support classes.

## Artifacts

- `endpoint-channel-propagation-probe/summary.json`
- `endpoint-channel-propagation-probe/summary.html`

## Verification

```text
cargo fmt --all
cargo test -p aic-data endpoint_channel_probe -- --nocapture
cargo test -p aic-cli parses_endpoint_channel_propagation_probe
cargo check --workspace
cargo test --workspace
cargo build --release -p aic-cli
git diff --check
```
