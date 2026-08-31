# Heavy Xiranite Shared Boundary Terminal Network Cliff Report

## Outcome

The first v4 performance cliff occurs when phase 0 grows from one commodity network to two. All
three single-network models found and proved an optimal layout in 0.85 to 2.53 seconds. All three
two-network models found no first incumbent in 5 seconds. The full three-network model behaved the
same way.

The smallest failing model is the pair of Xiranite powder belt networks. It has only 656 more
variables than the largest successful single-network model, and its isolated peak RSS is slightly
lower. Raw variable count and memory consumption therefore do not explain this cliff.

The decisive structural change is that a second commodity turns fixed-count Boolean route state
into coupled multi-valued flow and item state, while sharply increasing placement-routing
incidences and topology/capacity relations. This experiment locates that boundary but does not yet
identify whether hard feasibility propagation or lexicographic optimization search is the dominant
cause.

## Falsifiable Question And Method

The experiment asked which smallest exact network composition changes Heavy Xiranite phase 0 from
finding a complete incumbent within 5,000 ms to finding none.

Every case used:

- the unchanged `joint-shared-boundary-terminals-canonical-occupancy-v4` formulation;
- the same 12 by 12 caller-supplied diagnostic ceiling;
- the same phase-0 facility and complete placement, rotation, port, boundary, and route domains;
- a freshly rebuilt model containing only the selected logical commodity networks;
- an independent optimized release process and 5,000 ms search budget;
- `/usr/bin/time -l` peak RSS on macOS;
- automatic JSON and self-contained HTML for success or failure.

No excluded network remained as zero-flow state. No placement, port, boundary terminal, or route was
fixed. No heuristic or fallback was used.

## Result Matrix

Objective vectors are `(used area, physical transport tiles, route turns)`.

| Case | Networks | Requirements | Variables | Log2 volume | Constraints | Terms | First incumbent | Result | RSS |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: |
| `single-0` | 1 belt | 1 | 9,714 | 11,002.07 | 26,402 | 82,001 | 1,165 ms | optimal `(30,1,0)` | 78.69 MiB |
| `single-1` | 1 belt | 2 | 10,314 | 10,823.73 | 28,473 | 90,227 | 2,533 ms | optimal `(30,2,0)` | 85.27 MiB |
| `single-2` | 1 pipe | 1 | 9,598 | 10,712.18 | 26,062 | 80,789 | 846 ms | optimal `(30,1,0)` | 80.25 MiB |
| `pair-0-1` | 2 belt | 3 | 10,970 | 12,796.29 | 33,256 | 109,133 | none | unknown | 84.81 MiB |
| `pair-0-2` | 1 belt + 1 pipe | 2 | 18,326 | 20,686.62 | 50,989 | 159,247 | none | unknown | 113.42 MiB |
| `pair-1-2` | 1 belt + 1 pipe | 3 | 18,926 | 20,508.28 | 53,060 | 167,473 | none | unknown | 115.50 MiB |
| `full` | 2 belt + 1 pipe | 4 | 19,582 | 22,480.84 | 57,843 | 186,379 | none | unknown | 137.89 MiB |

Every successful witness passed independent validation and was proven optimal. Every unsuccessful
case is `unknown`, with no incumbent, proof, validation, objective, or reported solver bound. The
current report schema does not expose a best bound or optimality gap for these no-incumbent runs.

## First Cliff Delta

`single-1` is the largest successful one-network model. Adding the enriched-powder belt network
produces `pair-0-1`, the smallest unsolved two-network model. Both use only the belt physical layer,
so their cell, arc, branch, bridge, turn, and objective grid sizes are identical.

| Metric | `single-1` | `pair-0-1` | Delta |
| --- | ---: | ---: | ---: |
| Total variables | 10,314 | 10,970 | +656 (+6.36%) |
| Boolean variables | 10,002 | 9,402 | -600 |
| Integer variables | 312 | 1,568 | +1,256 (+402.56%) |
| Log2 domain volume | 10,823.73 | 12,796.29 | +1,972.56 (+18.22%) |
| Constraints | 28,473 | 33,256 | +4,783 (+16.80%) |
| Terms | 90,227 | 109,133 | +18,906 (+20.95%) |
| Placement-routing constraints | 3,040 | 5,872 | +2,832 (+93.16%) |
| Placement-routing incidences | 9,300 | 23,350 | +14,050 (+151.08%) |
| Build time | 54 ms | 68 ms | +14 ms |
| Peak RSS | 85.27 MiB | 84.81 MiB | -0.46 MiB (-0.53%) |
| First incumbent | 2,533 ms | none in 5,000 ms | cliff |

The model is not failing because another dense grid was created. The same 144 route-cell variables,
528 route-arc variables, 1,152 branch-component variables, 144 bridge variables, and 2,129
objective variables exist on both sides of the cliff.

## Variable-Domain Change

Only two variable families add most of the 656 new variables:

| Family | Variable delta | Log2-volume delta | Meaning |
| --- | ---: | ---: | --- |
| Endpoint geometry | +342 | +391.03 | additional facility-port choices for the new network |
| Boundary terminal | +313 | +321.17 | one additional external terminal over the complete boundary domain |

More important, two fixed-size families change type without increasing their variable count:

| Family | Count | `single-1` | `pair-0-1` | Log2-volume delta |
| --- | ---: | --- | --- | ---: |
| Flow | 528 | Boolean | integer | +836.86 |
| Arm item | 720 | Boolean | integer item identity | +421.17 |

The single powder network has unit line capacity in its normalized scale, so each arc-flow variable
is Boolean. Combining it with the enriched-powder network uses a larger common flow scale and line
capacity; the same 528 arc states become bounded integers. Likewise, one item can be represented by
presence, while two items require every arm to carry a multi-valued item identity. These two type
changes account for 1,258.03 of the 1,972.56 added log-domain volume.

## Constraint And Coupling Change

The largest added term families are:

| Family | Constraint delta | Term delta |
| --- | ---: | ---: |
| Branch topology | +1,152 | +9,216 |
| Line capacity | +1,104 | +2,808 |
| Bridge crossing | +936 | +2,400 |
| Terminal presence | +648 | +1,944 |
| Item assignment | +648 | +1,296 |

The factor graph shows a larger jump than raw model size: placement-routing constraints almost
double and their incidences grow by 151.08%. A second network therefore increases how many
decisions must agree, not merely how many decisions exist.

## What The Matrix Rules Out

- **Raw variable count:** ruled out as a sufficient explanation. A 6.36% increase crosses the
  cliff, while the smallest failing model uses only 10,970 variables.
- **Memory exhaustion:** ruled out. `pair-0-1` uses 0.53% less peak RSS than `single-1`.
- **A second physical transport grid:** ruled out as necessary. `pair-0-1` uses the same one belt
  layer and identical route-grid variable counts as both successful belt singles.
- **Belt/pipe interaction:** ruled out as necessary. The two-belt pair already fails.
- **Same-layer multi-item identity:** ruled out as necessary for every failure. Both belt-plus-pipe
  pairs also fail even though each item occupies a different physical layer.
- **Model construction:** ruled out as the main cost. The smallest failing pair builds in 68 ms.
- **The full third network:** ruled out as the first cliff. Every pair has already crossed it.

The common boundary supported by this matrix is two simultaneous commodity networks coupled through
one facility placement and one global used-geometry objective. The matrix cannot yet distinguish
whether the first-incumbent failure is caused primarily by hard multi-network feasibility
propagation or by the lexicographic optimization search wrapped around that feasible set.

## One Next Discriminating Experiment

Run `pair-0-1` in two research-only modes with identical hard constraints and budgets:

1. the current faithful lexicographic optimization baseline;
2. feasibility search with objective optimization disabled but every objective-definition and hard
   game constraint retained where possible.

This ablation changes no legal solution, but its second result is diagnostic-only because it does
not enforce solution quality. If feasibility mode finds an incumbent quickly, the immediate cliff
is objective/search orchestration. If it also finds none, the blocker lies in hard placement,
terminal, flow, item, topology, and capacity propagation and should be decomposed there next.

Do not implement this ablation until the user reviews the present boundary.

## Verification

- `cargo fmt --all`
- `cargo check --workspace`
- `cargo test --workspace`: 177 tests passed
- `cargo build --release --workspace`
- seven isolated optimized release processes with equal 5,000 ms search budgets
- JSON and self-contained HTML for every success and failure
- process-isolated `/usr/bin/time -l` peak RSS
- no production solver change, heuristic, fixed solver decision, or fallback

## Artifacts

- Experiment contract:
  `docs/designs/shared-boundary-terminal-phase0-cliff-diagnosis.md`
- Machine-readable normalized matrix:
  `docs/benchmarks/heavy-xiranite-shared-boundary-terminal-cliff/matrix.json`
- Raw per-case JSON, HTML, and single-case summaries:
  `docs/benchmarks/heavy-xiranite-shared-boundary-terminal-cliff/isolated/`

## Decision Boundary

The network-composition cliff is identified and committed as a diagnostic checkpoint. The exact
v4 production model remains unchanged. Pause here for user review before implementing the proposed
feasibility-versus-optimization ablation.
