# Heavy Xiranite Minimum-Rate Network-Interaction Decomposition Report 6

## Question

Why does the circulation-permitted 12 by 12 phase-zero model fail to find a first complete
assignment when it contains three commodity networks, even though the same phase contains only one
facility?

This checkpoint follows the experiment approved at the end of Report 5. It isolates every network
and pair, then removes constraint families or fixes already-known decisions only in temporary
diagnostic models. It does not change the production solver.

## Controlled Method

- Workload: `heavy-xiranite-minimum-rate`
- Cumulative SCC phase: 0
- Request ceiling: 12 by 12
- Solver: Pumpkin 0.5, release build
- Default search budget: 5,000 ms
- Search: Pumpkin's unchanged default brancher
- Production formulation: `joint-lexicographic-layout-v5`
- Production solver changes: none

The temporary diagnostic build could select a subset of phase-zero networks, omit objectives,
branch topology, or bridge/collision blocks, and constrain placement or port decisions to values
observed in a complete single-network solution. These relaxations and fixed decisions change the
feasible set. They are causal probes only and are not proposed solver paths or accepted heuristics.

All four original logical endpoint-choice constraints remained in the network-subset cases. This
kept the shared facility and port-choice structure visible while selecting which physical routing
networks were constructed.

## Phase-Zero Networks

| Index | Network | Transport | Terminals | Route requirements | Role |
| ---: | --- | --- | ---: | ---: | --- |
| 0 | `item-xiranite-enr-powder` | Belt | 2 | 1 | Final product output to an external target |
| 1 | `item-xiranite-powder` | Belt | 4 | 2 | Two frontier inputs into the facility |
| 2 | `item-liquid-xiranite-poly` | Pipe | 2 | 1 | One frontier input into the facility |

## Complete Single and Pair Models

Every single network reaches and proves the complete five-stage optimum within five seconds. Every
pair fails before its first incumbent.

| Networks | Variables | Constraints | Incidences | First incumbent | Final result | Objective vector |
| --- | ---: | ---: | ---: | ---: | --- | --- |
| Output belt only (0) | 12,497 | 37,232 | 139,255 | 2,195 ms | optimal, 2,721 ms | `[30, 1, 0, 6, 0]` |
| Input belt only (1) | 12,497 | 43,948 | 168,375 | 1,909 ms | optimal, 2,237 ms | `[30, 2, 0, 6, 0]` |
| Input pipe only (2) | 12,497 | 31,840 | 115,959 | 3,643 ms | optimal, 3,932 ms | `[30, 1, 0, 6, 0]` |
| Output belt + input belt (0, 1) | 19,417 | 72,132 | 257,383 | none | `unknown`, 5,000 ms | none |
| Output belt + input pipe (0, 2) | 19,561 | 60,168 | 205,255 | none | `unknown`, 5,000 ms | none |
| Input belt + input pipe (1, 2) | 19,561 | 66,884 | 234,375 | none | `unknown`, 5,000 ms | none |

This disproves the hypothesis that one intrinsically difficult four-terminal network explains the
failure. The four-terminal input belt network is the fastest complete single-network case. The
first discontinuity is composition: adding any second complete network crosses the five-second
first-incumbent boundary.

## Constraint-Family Ablation

The next table removes optimization first, then branch topology and bridge/collision modeling.
`Minimal` retains placement, all endpoint choices, grid arcs and flows, conservation, terminal
presence, directional arms, line capacity, route-cell activation, and opposite-direction
exclusion.

| Pair | Satisfaction core | No branch | No bridge/collision | Minimal |
| --- | --- | --- | --- | --- |
| Output belt + input belt (0, 1) | `unknown` | `unknown` | `unknown` | `unknown` |
| Output belt + input pipe (0, 2) | `unknown` | `unknown` | `unknown` | `unknown` |
| Input belt + input pipe (1, 2) | `unknown` | `unknown` | `unknown` | feasible, 503 ms |

The minimal-model structural counts are:

| Pair | Variables | Constraints | Incidences | Result |
| --- | ---: | ---: | ---: | --- |
| Output belt + input belt (0, 1) | 10,848 | 27,905 | 89,344 | `unknown`, 5,000 ms |
| Output belt + input pipe (0, 2) | 10,848 | 19,829 | 61,120 | `unknown`, 5,000 ms |
| Input belt + input pipe (1, 2) | 10,848 | 24,305 | 76,800 | feasible, 503 ms |

The smallest pair by both constraints and incidences is the one that remains hard. Raw model size
therefore does not predict this boundary. The output network's direction and its interaction with
another routing assignment matter more than the number of terminals or constraints.

## Placement and Port Coupling

The temporary fixed decisions use placement `(x=1, y=0, rotation=270)` and facility ports selected
by complete single-network solutions. They only diagnose where search time is spent.

| Minimal pair | Nothing fixed | Placement fixed | Ports fixed | Both fixed |
| --- | --- | --- | --- | --- |
| Output belt + input belt (0, 1) | `unknown` | feasible, 2,188 ms | feasible, 1,451 ms | feasible, 1,928 ms |
| Output belt + input pipe (0, 2) | `unknown` | `unknown` | `unknown` | `unknown` |

For the two-belt pair, fixing either side of the placement-to-port choice is enough to cross the
five-second boundary. This is direct evidence that the joint facility-candidate and directional
port-option encoding is the first blocker for that pair.

The belt-pipe pair is different. With placement and ports both fixed, it remains `unknown` at five
seconds, but finds a satisfaction assignment at 8,171 ms under a 30-second budget. Reversing the
network construction order still returns `unknown` at 5,001 ms. This establishes that the model is
feasible and that simple network ordering does not remove the threshold.

After objectives, branches, bridges, collision, placement choice, and port choice are removed or
fixed, the remaining unfixed state consists primarily of each network's route arcs, integer flows,
route cells, terminal-presence variables, and directional arms. The two networks have no belt-pipe
collision relation in this diagnostic. Pumpkin is therefore searching the product of two large
routing assignment spaces even though the logical terminals are already known.

## Root-Cause Model

The evidence supports two separate first-incumbent blockers rather than one:

1. **Placement-port coupling for same-layer output and input routing.** The two-belt pair becomes
   tractable when either the placement or the selected ports are fixed. The current endpoint model
   expands every facility candidate into directional port options and ties every option back to the
   shared placement variable.
2. **Multiplicative routing state across networks.** The belt-pipe pair remains hard after those
   logical choices are fixed and after all cross-layer collision and objective coupling is absent.
   It eventually solves at 8,171 ms, so this is search cost rather than infeasibility.

Circulation-permitted routing magnifies the second blocker. Every selected arc must carry positive
flow, but conservation permits closed positive-flow circulations that contribute no terminal
delivery. A single network has many such assignments; independent network routing spaces multiply
those alternatives. The default brancher can enter those spaces instead of quickly choosing the
all-unused state for irrelevant arcs.

This explanation is consistent with all measured facts:

- removing acyclicity made one network tractable;
- every complete single network is now optimal within four seconds;
- pairs fail discontinuously rather than in proportion to constraint count;
- fixing placement and ports is not sufficient for the hard belt-pipe pair;
- the fixed belt-pipe pair becomes feasible with more time;
- reversing network order does not cross the five-second boundary.

## Newly Observed Correctness Conflict

The diagnostic also reached the revisit condition established when circulation was approved.
With a fixed placement, the single input-belt and input-pipe optimizations found and proved objective
assignments, but independent witness validation rejected both with:

```text
invalid-integrated-layout-witness:
every active transport cell must lie on a directed supply-to-demand flow path
```

Under the v5 solver semantics, a closed positive-flow circulation is legal. Under the current
witness semantics, the same circulation is illegal. The production model and independent validator
therefore no longer recognize the same solution set. This is not merely visualization noise: a
solver result can be proven optimal and then reported as `unknown` because validation rejects a
circulation that the model deliberately permits.

## Decision Gate

Two decisions are required before the next implementation or diagnostic slice:

1. **Validation semantics:** either update the validator to accept conserved net-zero circulation,
   matching the approved v5 model, or reintroduce a model/objective rule that makes such circulation
   illegal or strictly dominated.
2. **Next causal probe:** explicitly approve a diagnostic that fixes irrelevant route arcs to zero
   for these frontier-dangling phase-zero connections. This would not be a production heuristic; it
   would test whether circulation/route-state freedom is the remaining cause. Because it fixes
   routing decisions and excludes legal v5 assignments, it requires explicit approval under the
   project's heuristic policy.

No automatic reduction, routing template, corridor, fixed placement, fixed port, or zero-arc rule
has been added to the production solver.
