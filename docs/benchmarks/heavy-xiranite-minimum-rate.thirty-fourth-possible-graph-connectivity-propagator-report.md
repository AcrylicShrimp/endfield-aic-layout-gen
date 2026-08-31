# Heavy Xiranite Minimum-Rate Possible-Graph Connectivity Propagator Report

## Result

The first diagnostic-only Pumpkin custom propagator improves the controlled cumulative Phase 2
first-witness search without adding solver decision variables or excluding any legal route.

The propagator asks one global question for each material network:

> Given the values still available in the current domains, can this demand terminal still be
> reached from any compatible supply terminal?

If the answer is no, the propagator removes only that demand-terminal option. The existing exact
model still decides facility placement, ports, physical arcs, item assignment, flow, topology,
capacity, and collision.

In two release-mode 30-second-budget repetitions, the custom case found its first validated witness
in `4.427-4.433 s`. The unchanged baseline needed `5.497-5.693 s`. This is a measured first-witness
reduction of `19.5-22.1%`. With a strict five-second case budget, the baseline returned `unknown`
while the custom case found a validated witness in `4.414 s`.

This is the first positive result from adding explicit global source-to-demand connectivity
knowledge. It is not yet suitable for the production solve path because the implementation wakes
and rebuilds its possible graph far too often.

## Contract

The exactness contract is documented in
`docs/designs/possible-route-connectivity-propagator-experiment.md`.

- The propagator is enabled only by the dedicated research command.
- It adds no placement, port, route, path, parent, depth, or objective variables.
- It reads the existing arc-selection, arm-item, and terminal-selection domains.
- A possible directed arc exists only while the arc can be selected and both endpoint arms can
  still carry the material.
- A demand option is removed only when no path exists from any still-possible compatible supply.
- Bridge traversal is deliberately over-approximated. This can miss pruning but cannot remove a
  legal route.
- Explanations contain the currently excluded supplies and one exclusion predicate for every arc
  absent from the possible graph.
- No route shape, port, placement, corridor, or path candidate is chosen outside the solver.

## Controlled Problem

All cases use the same cumulative SCC Phase 2 problem:

- used dimensions fixed exactly to `12x12`;
- three facility placements fixed to the same validated reference;
- all facility and external terminals fixed to that reference;
- eight same-item commodity networks and ten logical requirements;
- feasibility-only search;
- every remaining routing, item, flow, topology, capacity, and collision decision left free.

Fixing placement and terminal choices is a diagnostic ablation, not a production strategy. It keeps
the experiment focused on whether global route connectivity propagation helps the previously
isolated routing cliff.

## Repeated Measurements

| Run | Case budget | Baseline first witness | Custom first witness | Result |
| --- | ---: | ---: | ---: | --- |
| Strict boundary | 5 s | `unknown` | 4,414 ms | custom crosses the five-second boundary |
| Recorded 30 s | 30 s | 5,497 ms | 4,427 ms | custom 19.5% faster |
| Repetition 30 s | 30 s | 5,693 ms | 4,433 ms | custom 22.1% faster |

The custom case followed the same search trace in all three runs: its counters and witness objective
were identical. Baseline time varied by 196 ms across the two completed runs.

The comparison is first-witness only. The cases stop after one feasible result, so their incidental
witness objectives are not a solution-quality comparison:

| Case | Area | Transport tiles | Turns | Maximum side | Components |
| --- | ---: | ---: | ---: | ---: | ---: |
| Baseline witness | 144 | 96 | 37 | 12 | 13 |
| Custom witness | 144 | 99 | 45 | 12 | 15 |

Both witnesses pass the unchanged independent layout validator.

## Model Delta

| Metric | Baseline | Custom | Delta |
| --- | ---: | ---: | ---: |
| Variables | 23,972 | 23,972 | 0 |
| Constraints | 82,648 | 82,656 | +8 global propagators |
| Recorded incidences | 285,128 | 299,544 | +14,416 |
| Build time, recorded run | 198 ms | 199 ms | +1 ms |

The eight propagators subscribe to 18,640 variable incidences before de-duplication within each
propagator. The largest material-network propagator observes 2,320 unique variables.

## Runtime Breakdown

The custom trace is identical in all recorded runs:

| Counter | Value |
| --- | ---: |
| Propagator executions | 144,775 |
| Directed arcs scanned | 76,441,200 |
| Demand options checked | 46,762,456 |
| Unsupported selected-demand conflicts | 921 |
| Maximum explanation predicates | 1,156 |

This is approximately 83,000 arc scans and 50,800 demand-option checks per useful conflict. The
global inference is valuable enough to improve total search time despite that overhead, but the
implementation is computationally wasteful.

The next measured blocker is therefore no longer a missing connectivity concept. It is the custom
propagator's coarse event subscription and full graph rebuild:

1. every relevant integer-domain event wakes a whole material-network propagator;
2. every wake scans every physical arc for that material;
3. adjacency and reachability are rebuilt from scratch even when the event cannot remove an arc or
   supply;
4. large eager explanations enumerate the full absent-arc set.

## Improvements Established Before This Checkpoint

The current result builds on the following exact, semantics-preserving changes and measurements:

1. separated the search ceiling from the actual used blueprint bounding box;
2. made used area, physical transport tiles, and turns the ordered objective vector;
3. replaced per-network dense routing grids with shared belt and pipe physical layers;
4. separated facility placement and port selection instead of flattening their Cartesian product;
5. added exact shared facility-versus-transport occupancy propagation;
6. modeled external connections through the shared transport layer rather than a separate route
   shape generator;
7. partitioned exact used dimensions outside the solver and shared incumbents across independent
   cases;
8. measured cumulative SCC growth and located the first current cliff at Phase 2;
9. rejected transport-tile caps as the primary Phase 2 remedy;
10. rejected a dense declarative parent/depth connectivity forest after it increased variables,
    constraints, and first-witness time;
11. demonstrated that a variable-free possible-graph connectivity propagator can cross the current
    five-second boundary.

These results do not establish the final solver architecture. They narrow the next exact experiment.

## Recommended Next Experiment

Keep the same possible-graph inference and exact solution set, but make propagation selective and
incremental:

1. subscribe only to events that can remove an arc, remove the material from an arm, or remove a
   possible supply;
2. avoid waking on domain changes that cannot invalidate an existing possible path;
3. cache or incrementally maintain per-network graph state instead of rebuilding all adjacency
   lists on every event;
4. record wakeups avoided, arcs scanned, explanation size, first-witness time, and memory;
5. compare against both the unchanged baseline and this from-scratch custom propagator.

This remains a semantics-preserving reformulation. It does not preselect a path or constrain the
solver to a hand-written routing pattern. Mandatory-cut forcing, custom branching, and production
enablement remain out of scope until this checkpoint is reviewed.

## Artifacts

- `heavy-xiranite-phase2-possible-graph-connectivity-5s/summary.html`
- `heavy-xiranite-phase2-possible-graph-connectivity-5s/summary.json`
- `heavy-xiranite-phase2-possible-graph-connectivity-30s/summary.html`
- `heavy-xiranite-phase2-possible-graph-connectivity-30s/summary.json`

The release-mode command is:

```bash
target/release/aic-cli research diagnose-phase2-possible-graph-connectivity \
  --workload data/benchmarks/heavy-xiranite-minimum-rate.workload.json \
  --workspace-root . \
  --placement-request data/benchmarks/requests/placement.12x12.request.json \
  --target-phase 2 \
  --used-width 12 \
  --used-height 12 \
  --facility-x 0 \
  --facility-y 1 \
  --port-assignment-index 5 \
  --prefix-case-time-limit-ms 5000 \
  --reference-time-limit-ms 30000 \
  --case-time-limit-ms 5000 \
  --output-dir docs/benchmarks/heavy-xiranite-phase2-possible-graph-connectivity-5s
```
