# Heavy Xiranite Factored Endpoint Report 9

## Question

What changes when the shared transport-layer formulation replaces one Boolean variable for every
`placement candidate times port` pair with independent placement and port choices plus exact
derived terminal geometry?

This checkpoint implements and measures that representation. It does not replace the production
dense solver or the flattened shared-layer experiment.

## Exact Encoding

The flattened reference creates one endpoint Boolean for every legal placement-port tuple. The
factored formulation instead uses:

- the existing one-hot placement candidates;
- one integer placement-index variable tied exactly to those candidates;
- one integer port-index variable per logical facility endpoint;
- `combined = placement_index * port_count + port_index`;
- a solver-native element constraint mapping `combined` to the connection cell and facility-side
  arm direction; and
- equality literals for each reachable terminal geometry consumed by routing constraints.

The paired external terminal reuses the same geometry literal with the direction reversed. It does
not create another physical-coordinate choice.

Invalid placement-port entries map to `-1`, which is outside the geometry variable's domain. The
element propagator therefore removes exactly the illegal boundary tuples. No legal tuple is
removed, and no placement, rotation, port, or route is selected before solving.

Pumpkin 0.5's generic positive-table constraint was inspected and deliberately not used. Its
implementation creates one Boolean row selector per tuple, which would recreate the Cartesian
state behind a smaller-looking public variable list. The element constraint uses a dedicated
propagator and does not introduce those row selectors.

## Correctness Check

The existing small two-belt-item case was solved by:

1. the dense per-network-grid formulation;
2. the shared-layer formulation with flattened endpoints; and
3. the shared-layer formulation with factored endpoints.

All three produced a complete independently validated witness and the same proven lexicographic
objective. This is focused regression evidence, not a general proof of equivalence.

## Controlled Heavy Xiranite Run

- Workload: `heavy-xiranite-minimum-rate`
- Cumulative SCC phase: 0
- Facility count: 1
- Commodity networks: 3
- Logical route requirements: 4
- Network terminals: 8
- Request ceiling: 12 by 12
- Solver: Pumpkin 0.5, release build
- Search budget: 5,000 ms independently per formulation
- Search configuration: unchanged Pumpkin search
- Heuristic restriction or fallback: none

The committed artifacts are:

- `heavy-xiranite-factored-endpoint-comparison/comparison.json`
- `heavy-xiranite-factored-endpoint-comparison/flattened.html`
- `heavy-xiranite-factored-endpoint-comparison/factored.html`

They can be regenerated with:

```bash
target/release/aic-cli research compare-first-phase-factored-endpoints \
  --workload data/benchmarks/heavy-xiranite-minimum-rate.workload.json \
  --workspace-root . \
  --placement-request data/benchmarks/requests/placement.12x12.request.json \
  --time-limit-ms 5000 \
  --output-dir docs/benchmarks/heavy-xiranite-factored-endpoint-comparison
```

## Result

| Metric | Flattened endpoint | Factored endpoint | Change |
| --- | ---: | ---: | ---: |
| Endpoint states | 3,584 | 1,188 | -2,396 (-66.9%) |
| Total variables | 20,137 | 17,742 | -2,395 (-11.9%) |
| Boolean variables | 17,778 | 15,370 | -2,408 (-13.5%) |
| Integer variables | 2,359 | 2,372 | +13 (+0.6%) |
| Log2 domain volume | 22,829.76 | 20,512.37 | -2,317.39 (-10.2%) |
| Constraints | 74,148 | 61,089 | -13,059 (-17.6%) |
| Constraint terms | 282,995 | 217,287 | -65,708 (-23.2%) |
| Factor-graph incidences | 279,411 | 216,111 | -63,300 (-22.7%) |
| Placement-routing incidences | 128,220 | 72,836 | -55,384 (-43.2%) |
| Model construction | 150 ms | 113 ms | -37 ms (-24.7%) |
| Search | 5,001 ms | 5,000 ms | equal budget |
| First incumbent | none | none | unchanged |

Both five-second results are `unknown`, not infeasible. A separate 15-second release run of the
factored formulation also returned `unknown` with no first incumbent.

The user-visible endpoint count includes eight actual endpoint/combined-index variables and 1,180
derived geometry literals. The flattened formulation has 3,584 placement-port tuple Booleans.

## Peak RSS Probe

Each formulation was run alone in a separate release process with the same 12 by 12 input and
5,000 ms search budget. macOS `/usr/bin/time -l` reported:

| Formulation | Maximum RSS | Peak memory footprint |
| --- | ---: | ---: |
| Flattened | 151,699,456 bytes (144.7 MiB) | 148,537,872 bytes |
| Factored | 136,265,728 bytes (130.0 MiB) | 133,104,144 bytes |
| Change | -15,433,728 bytes (-10.2%) | -15,433,728 bytes (-10.4%) |

This is one observation per formulation, not a timing or memory distribution. It confirms that the
structural reduction produces a real process-memory reduction rather than only changing recorder
accounting.

## Remaining Cost

The endpoint factorization removes 1,016 of the 1,024 endpoint-link constraints and substantially
shrinks terminal-dependent terms. The largest remaining factored-model families by terms are:

| Constraint family | Constraints | Terms |
| --- | ---: | ---: |
| Branch topology | 14,688 | 64,416 |
| Turn definition | 10,912 | 27,520 |
| Item assignment | 7,152 | 23,712 |
| Used geometry | 6,858 | 20,282 |
| Bridge crossing | 4,344 | 13,344 |
| Transport collision | 288 | 13,088 |
| Terminal presence | 4,656 | 9,360 |
| Line capacity | 3,292 | 8,812 |

The factor graph remains one connected component. Branch components to branch-topology constraints
are now the largest family incidence, followed by objective-to-turn and route-arm-to-branch
topology. Endpoint representation was therefore a substantial cost, but it was not the sole
first-incumbent blocker.

## Conclusion And Pause Point

Independent placement and port state is a successful exact reformulation by every measured static
criterion. It cuts endpoint state by two thirds, direct placement-routing coupling by more than
two fifths, model construction by about one quarter, and peak RSS by about one tenth. It should
remain the preferred endpoint representation for the shared-layer experiment.

It does not cross the phase-zero search cliff at five or fifteen seconds. Stop here before changing
another formulation family. The next interactive research decision is whether to isolate branch
topology, item assignment, turn-objective state, or their interaction as the next remaining exact
search blocker.
