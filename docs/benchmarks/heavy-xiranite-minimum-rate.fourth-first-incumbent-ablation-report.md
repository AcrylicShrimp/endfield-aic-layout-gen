# Heavy Xiranite Minimum-Rate First-Incumbent Ablation Report 4

## Question

Assuming the 12 by 12 phase-zero problem has a feasible solution, what is the smallest tested model
feature that prevents Pumpkin from finding its first solution within five seconds?

This checkpoint diagnoses the first satisfaction search. It does not adopt a reduction or alter
the production solver.

## Controlled Method

- Workload: `heavy-xiranite-minimum-rate`
- Cumulative phase: 0, containing one facility
- Diagnostic request ceiling: 12 by 12
- Solver: Pumpkin 0.5, release build
- Search budget: 5,000 ms per case
- Search: Pumpkin's unchanged default brancher
- Objective bound: none

Pumpkin's `LinearSatUnsat` procedure first calls unconstrained satisfaction before applying any
objective bound. A 60-second release run of the complete model still returned `unknown` with zero
incumbents. Removing the complete objective-auxiliary block also returned `unknown` after five
seconds. The experiment therefore continued inside the placement-routing core.

The ablation executable was built in a detached temporary worktree. It could stop model construction
at named boundaries, limit the number of modeled commodity networks, or relax one core feature.
It did not fix a placement, port, route, or component choice. The diagnostic changes were not copied
into the repository or production binary.

Every relaxed case changes the feasible set and is evidence only. In particular, removing
acyclicity permits circulation and is not a proposed production fix.

## Cumulative Boundary

The first network is a belt network for `item-xiranite-enr-powder` with two terminals. Endpoint-only
cases still include all four phase-zero route requirements' facility endpoint choices. A “full
core” includes flow, route occupancy, directional arms, line capacity, acyclicity, branch-component
topology, bridges, and transport collision, but excludes objective auxiliaries.

| Cumulative model | Variables | Constraints | Incidences | First solution |
| --- | ---: | ---: | ---: | ---: |
| Placement only | 256 | 145 | 6,656 | satisfiable, 0 ms |
| Placement + all endpoint choices | 3,840 | 1,173 | 14,848 | satisfiable, 1 ms |
| Above + one full commodity network | 10,368 | 25,293 | 104,624 | `unknown`, 5,000 ms |
| Above + two full commodity networks | 15,456 | 54,977 | 208,416 | `unknown`, 5,001 ms |
| Above + all three full commodity networks | 20,544 | 72,553 | 259,792 | `unknown`, 5,001 ms |
| Complete model including objectives | 26,913 | 96,652 | 328,135 | `unknown`, 5,001 ms |

The first five-second boundary appears when one physical commodity network is added. It does not
require belt-pipe interaction, multiple networks, or objective optimization.

## Single-Network Feature Ablation

| One-network core variant | Variables | Constraints | Incidences | First solution |
| --- | ---: | ---: | ---: | ---: |
| Complete core | 10,368 | 25,293 | 104,624 | `unknown`, 5,000 ms |
| Remove only acyclicity ordering | 10,224 | 24,765 | 103,040 | satisfiable, 2,695–3,044 ms |
| Remove only branch-component topology | 9,216 | 17,373 | 70,704 | `unknown`, 5,000 ms |
| Remove only bridge/collision model | 8,640 | 19,597 | 77,456 | `unknown`, 5,000 ms |
| Keep acyclicity; remove branch and bridge/collision | 7,488 | 12,829 | 45,840 | `unknown`, 5,000 ms |
| Remove acyclicity, branch, and bridge/collision | 7,344 | 12,301 | 44,256 | satisfiable, 289–307 ms |

The decisive paired comparison is the last two rows. They differ only by 144 order variables, 528
acyclicity constraints, and 1,584 factor-graph incidences. With that block present, three repeated
runs all timed out. Without it, three repeated runs all found a relaxed solution in 289, 299, and
307 ms. The same direction held with branch and bridge/collision modeling retained: four complete-
core runs timed out, while four no-acyclicity runs found relaxed solutions in 2,695–3,044 ms
(median approximately 2,731 ms).

## Minimal Tested Feature

The smallest tested causal feature is the **current topological-order acyclicity encoding for one
commodity network**.

For a 12 by 12 network it creates one integer order variable per cell, each with domain 0 through
143. For every directed grid arc `u -> v`, it posts the equivalent of:

```text
selected(u, v) = 1  =>  order(v) >= order(u) + 1
```

The current linear form uses 144 as a big-M coefficient. When an arc is inactive the corresponding
ordering relation is almost unconstrained. Before any route is known, the model therefore contains
144 wide-domain, highly symmetric order variables coupled by 528 weak conditional inequalities.

Pumpkin's default brancher starts with no learned activity and falls back to random variable
selection and random domain splitting across all variables. The measured behavior is consequently
an interaction between this weak, symmetric encoding and generic search. It is not evidence that
the game rule “routes must not contain useless cycles” is inherently expensive.

This is a minimal result within the tested feature boundaries, not a proof that one particular
order constraint or variable is individually responsible.

## Multi-Network Check

Removing acyclicity is sufficient to cross the five-second boundary for one network, but not for
the complete three-network phase:

| Relaxed multi-network variant | Variables | Constraints | Incidences | First solution |
| --- | ---: | ---: | ---: | ---: |
| Two networks, no acyclicity | 15,168 | 53,921 | 205,248 | `unknown`, 5,000 ms |
| Two networks, no acyclicity/branch/bridge | 10,848 | 27,905 | 89,344 | `unknown`, 5,001 ms |
| Three networks, no acyclicity | 20,112 | 70,969 | 255,040 | `unknown`, 5,001 ms |
| Three networks, no acyclicity/branch/bridge | 14,352 | 35,433 | 106,208 | `unknown`, 5,000 ms |

The acyclicity encoding is the first isolated five-second trigger, but it is not the only scaling
problem. Adding the second network creates another first-solution barrier even in the most relaxed
tested routing core. That later barrier remains unresolved.

## Supported Conclusion

Under the stated feasibility assumption, the answer is:

> One 12 by 12 commodity network plus the current per-cell topological-order acyclicity encoding is
> already sufficient to prevent Pumpkin's default search from finding a first solution in five
> seconds.

Removing objectives, branching components, bridges, or collision constraints individually does not
cross the boundary. Removing only the acyclicity block does.

The next exact research target should be a semantics-preserving replacement or strengthening of
the acyclicity formulation. Any comparison must retain all legal cycle-free routes, measure first
incumbent time separately from proof time, and then return to the unresolved two-network barrier.
