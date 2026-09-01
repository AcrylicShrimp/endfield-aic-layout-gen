# Heavy Xiranite Phase 3 Crossing-Free Restriction Experiment

## Result

Forbidding every same-layer belt and pipe crossing did not remove the current Phase 3 first-witness
cliff. Four unrestricted control runs and four crossing-free runs each returned `Unknown` without a
first witness at five seconds. The predeclared thirty-second crossing-free observation also returned
`Unknown` without a witness.

The result does not prove that bridges are irrelevant. It rejects the narrower hypothesis that
bridge/crossing freedom alone immediately turns the current `16 x 16` Phase 3 model into an easy
witness problem.

## Hypothesis And Contract

The proposed progressive-relaxation path was:

```text
solve a crossing-free restricted model
  -> obtain one complete legal witness
  -> remove the restriction completely
  -> warm-start the unrestricted exact model from that witness
```

This report covers only the first stage. The restricted model is an auxiliary heuristic witness
generator because it excludes legal bridge-using layouts and may exclude the unrestricted optimum.
Its `Unknown` or `Infeasible` outcome is never interpreted as evidence about unrestricted
feasibility.

The controlled workload is the ordinary cumulative Phase 3 production graph at fixed used
dimensions `16 x 16`:

- four facilities;
- thirteen logical route requirements;
- eight shared commodity networks: three belt and five pipe;
- twenty-six network terminals, ten of them external;
- placement, rotation, ports, boundary terminals, routes, flow, splitters, convergers, and all
  objective-related state remain solver decisions;
- both arms use the same sparse endpoint-support, possible-connectivity, watched-demand chain,
  local-continuation, and guarded-intersection propagator stack;
- neither arm receives a prior witness hint.

The run order is counterbalanced and every run constructs a fresh model:

```text
A B B A  B A A B
```

`A` is unrestricted. `B` posts one equality over every bridge-selection Boolean:

```text
sum(all belt and pipe bridge_selected variables) = 0
```

All bridge variables are Boolean, so this is equivalent to fixing every bridge selection to zero.
Existing bridge-rotation equalities then force every rotation selection to zero. Belt and pipe
remain separate layers and may still occupy the same `(x,y)` cell.

## Build Certificate And Exact Delta

Every `B` build records a native certificate containing all `(transport, cell)` bridge variables.
The certificate proves:

- 512 observed bridge selections equals `256 grid cells x 2 active transport layers`;
- every bridge appears exactly once;
- one equality with 512 terms was posted;
- zero variables were added;
- ordinary witness validation is followed by an explicit zero-bridge component check.

The measured model delta exactly matches that certificate:

| Metric | A unrestricted | B crossing-free | Delta |
| --- | ---: | ---: | ---: |
| Variables | 64,471 | 64,471 | 0 |
| Constraints | 163,817 | 163,818 | +1 |
| Constraint terms | 635,619 | 636,131 | +512 |
| Factor-graph incidences | 626,019 | 626,531 | +512 |

All repeated A models are identical. All repeated B models, including the longer observation, are
identical. The A/B formulation,
ordinary model metrics, variable domains, and variable counts match. The declared crossing
restriction is the only measured structural difference: after removing the restriction family,
every other constraint-family and factor-incidence vector matches exactly. All interpretation gates
pass. No witness was found, so the separate hint-progression gate is false.

## Five-Second Results

| Run | Arm | Outcome | Build | Search | First witness | Decisions | Backtracks | Conflicts | Learned | Propagations |
| ---: | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | A | Unknown | 499 ms | 5,011 ms | none | 3,207 | 414 | 413 | 413 | 2,234,728 |
| 1 | B | Unknown | 474 ms | 5,012 ms | none | 4,514 | 549 | 548 | 548 | 2,087,774 |
| 2 | B | Unknown | 486 ms | 5,018 ms | none | 4,109 | 544 | 543 | 543 | 2,023,737 |
| 3 | A | Unknown | 507 ms | 5,009 ms | none | 3,197 | 404 | 403 | 403 | 2,229,394 |
| 4 | B | Unknown | 492 ms | 5,012 ms | none | 4,250 | 544 | 543 | 543 | 2,046,733 |
| 5 | A | Unknown | 518 ms | 5,010 ms | none | 3,126 | 351 | 350 | 350 | 2,179,253 |
| 6 | A | Unknown | 522 ms | 5,028 ms | none | 3,019 | 285 | 284 | 284 | 2,066,156 |
| 7 | B | Unknown | 562 ms | 5,011 ms | none | 3,955 | 541 | 540 | 540 | 1,953,446 |

Means:

| Metric | A | B | B / A |
| --- | ---: | ---: | ---: |
| Decisions | 3,137.25 | 4,207.00 | 1.34x |
| Conflicts | 362.50 | 543.50 | 1.50x |
| Propagations | 2,177,382.75 | 2,027,922.50 | 0.93x |

The deterministic portions of the B traces reproduce closely. A also remains in one narrow trace
family. The bridge-free restriction changes the search trajectory substantially, but not toward a
witness inside the five-second budget.

## Thirty-Second Observation

Because all four primary B runs were `Unknown`, the harness automatically ran the predeclared single
longer observation:

| Outcome | Build | Search | First witness | Decisions | Backtracks | Conflicts | Learned | Propagations |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Unknown | 493 ms | 30,030 ms | none | 24,905 | 4,604 | 4,603 | 4,603 | 10,428,008 |

This remains a censored performance observation. It is not an infeasibility proof for the
crossing-free subset.

## Interpretation

The restricted model visits more branch decisions and conflicts per five-second run while doing
slightly fewer total propagations. This is consistent with bridge removal causing the default
brancher to explore a different, more conflict-dense non-bridge topology space. It is not evidence
that adding bridges is beneficial in general, and it does not isolate one unique replacement
bottleneck.

What the experiment establishes:

1. bridge variables and crossing constraints are not the simple switch that makes the current full
   Phase 3 witness search easy;
2. the first-witness cliff persists for at least thirty seconds after every bridge is removed;
3. a crossing-free witness is unavailable, so the proposed witness-to-relaxed warm-start stage has
   no valid input in this experiment;
4. implementing a full shared-layer semantic hint mapper now would test a different generic hint
   hypothesis, not complete the proposed crossing-free path.

The current shared-layer warm start still maps placements only. Independent design review confirmed
that a full non-binding primary-semantic hint is implementable, but it should be a separate slice
only after a validated source witness exists.

## Improvements Preserved In This Baseline

This negative result is measured on the accumulated exact formulation, not the original dense
per-network grid:

- one shared belt layer and one shared pipe layer;
- independent placement state and port choice rather than flattened placement-port products;
- canonical physical facility occupancy coupled to both transport layers;
- sparse legal endpoint support rather than dense endpoint tables;
- exact boundary terminals inside the used dimensions;
- possible-route connectivity propagation;
- watched-demand unique-support chains;
- active local positive-flow continuation;
- guarded item-intersection propagation;
- exact fixed-dimension partitioning for this diagnostic only;
- structured solver counters and automatic failure HTML artifacts.

No placement, port, route, flow, or topology decision was replaced by a constructive heuristic.

## Next Action

Do not implement the full crossing-free witness hint path from this result because no restricted
witness exists. Replace the next top-down fixation experiment with a bottom-up formulation
diagnostic. This is not a multi-stage production solve and does not transfer or fix a solution from
one rung into the next. Each rung builds a fresh model for the same logical graph and cumulatively
enables one additional semantic block:

1. facility placement, rotation, non-overlap, and compact-area optimization;
2. compatible directional port assignment;
3. pipe routing;
4. belt routing without same-layer bridges;
5. complete same-layer bridge semantics.

The first transition that changes a solved model into a search cliff isolates the newly enabled
block and its coupling to the prior model. Model deltas, root-domain changes, and search counters can
then decide whether the next action should be an exact reformulation, stronger channeling, search
ordering, or a semantic custom propagator. The earlier 216-member guarded-clause breadth remains
available, but it is lower-information than locating the first cliff from a minimal formulation.

## Artifacts And Verification

Runtime artifacts:

```text
/tmp/aic-crossing-free-phase3.L1Wbrw/summary.json
/tmp/aic-crossing-free-phase3.L1Wbrw/summary.html
/tmp/aic-crossing-free-phase3.L1Wbrw/run-00-A.html
...
/tmp/aic-crossing-free-phase3.L1Wbrw/run-07-B.html
/tmp/aic-crossing-free-phase3.L1Wbrw/crossing-free-observation-30000ms.html
```

The CLI writes all success, failure, and timeout layouts automatically. The artifact directory is
1.7 MiB. The full counterbalanced experiment completed in 75,375 ms.

Verification performed for the slice:

```text
cargo fmt --all
cargo check --workspace
cargo test -p aic-data crossing_free --lib
cargo test -p aic-cli parses_integrated_endpoint_channel_comparison --bin aic-cli
cargo test -p aic-cli parses_crossing_free_restriction_comparison --bin aic-cli
cargo test --workspace
git diff --check
```

Three independent reviews separately examined soundness, runtime interpretation, and next-strategy
value. Their verified findings strengthened the long-observation identity gate, exact family delta,
Cartesian bridge certificate, invalid-witness and contradictory-outcome fail-closed checks, and
trusted-witness publication. The final runtime interpretation passed those gates.
