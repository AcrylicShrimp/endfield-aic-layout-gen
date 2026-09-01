# Heavy Xiranite Phase 3 Internal-Pipe Endpoint Continuation

## Result

The exact endpoint-continuation portfolio separates one proven source-direction exclusion from the
remaining Phase 3 first-feasible cliff.

The selected fully internal material network is
`network:pipe:item-liquid-xiranite-poly`. Its fixed source cell has two root-live outgoing arcs and
its fixed demand cell has three root-live incoming arcs. The complete exact portfolio therefore has
six source/demand cases:

| Case | Source arc | Earlier source arcs fixed to zero | Demand arc | Earlier demand arcs fixed to zero | Authoritative | Observation | Search ms | Decisions | Conflicts | Propagations |
| ---: | --- | ---: | --- | ---: | --- | --- | ---: | ---: | ---: | ---: |
| 0 | `48 -> 32` | 0 | `97 -> 113` | 0 | Proven infeasible | Proven infeasible | 150 | 0 | 1 | 245,264 |
| 1 | `48 -> 32` | 0 | `112 -> 113` | 1 | Proven infeasible | Proven infeasible | 154 | 0 | 1 | 245,362 |
| 2 | `48 -> 32` | 0 | `129 -> 113` | 2 | Proven infeasible | Proven infeasible | 152 | 0 | 1 | 245,348 |
| 3 | `48 -> 64` | 1 | `97 -> 113` | 0 | Unknown | Unknown | 5,007 | 43,257 | 4,434 | 4,369,327 |
| 4 | `48 -> 64` | 1 | `112 -> 113` | 1 | Unknown | Unknown | 5,007 | 42,617 | 2,408 | 4,218,892 |
| 5 | `48 -> 64` | 1 | `129 -> 113` | 2 | Unknown | Unknown | 5,007 | 41,262 | 2,902 | 4,385,093 |

All three cases selecting `48 -> 32` conflict during root propagation before any branch decision.
All three cases selecting `48 -> 64` remain cutoff-censored `Unknown`. No case finds an incumbent,
invalid witness, or evidence conflict. Every exactness and interpretation gate passes.

Because the six cases are a pairwise-disjoint exact cover, the three root proofs establish this
leaf-local fact:

> If the inherited key-24 parent leaf is feasible, its internal liquid-xiranite-poly source must
> send positive flow through `48 -> 64`; `48 -> 32` cannot be its first positive source arc.

This is a real exact pruning opportunity. It is not yet a general propagation rule. The current
artifact proves the exclusion under the complete inherited placement, port, external-boundary, and
other fixed-leaf context, but does not identify the smallest semantic reason for the root conflict.
The source cell is on the west boundary and the north-going arc appears to enter a corridor bounded
by the fixed facility footprint, so a dead-end connectivity explanation is plausible. It remains an
analyst inference rather than a solver-emitted cause certificate.

## Relationship to the Belt Result

The preceding belt experiment split two source and two demand continuations for the selected
final-product network. All four cases remained `Unknown`. This pipe experiment targets the same
local decision on the layer where Pumpkin makes its first native decision.

The pipe split is more informative:

- it proves half of the six exact cases infeasible;
- it reduces the source continuation to one legal direction inside this leaf;
- it leaves all three demand directions unresolved at the five-second cutoff;
- it still does not produce a feasible witness.

Therefore endpoint-local propagation is incomplete enough to expose a sound pruning fact, but the
remaining practical cliff lies beyond that fact.

## Exact Contract and Certificates

The run reuses the accepted endpoint-continuation formulation without changing code or model
semantics. The selected network has:

- one facility-owned source terminal at cell 48;
- one facility-owned demand terminal at cell 113;
- one positive source flow unit and one positive demand flow unit;
- singleton geometry and fixed terminal presence at root;
- two complete root-live source candidates and three complete root-live demand candidates.

Canonical case `i` selects candidate `i` with positive flow and fixes every earlier candidate to
zero. Later candidates remain free. The source and demand partitions are independently disjoint and
exhaustive, and their Cartesian product is an exact cover of the inherited parent feasible set.

All preflight and post-run gates pass:

- one-source/one-demand selected-network identity;
- singleton endpoint geometry and distinct endpoint cells;
- positive terminal flow and fixed terminal presence;
- mandatory-continuation semantic proof;
- pairwise-disjoint and exhaustive canonical cover;
- authoritative/observation certificate equality;
- complete nonselected external-boundary domains;
- four facility placements/rotations and fifteen facility ports fixed exactly as the parent;
- authoritative/observation exact-model and complexity identity within every case;
- exact child-versus-parent controlled-axis model deltas;
- no invalid witness or contradictory evidence.

## Controlled Model Change

The selected key-24 parent has 63,385 variables, 161,632 constraints, 618,978 incidences, and
242,663 placement-routing incidences. Child variables and placement-routing incidences remain
unchanged. Each child adds exactly two selected-positive constraints plus the canonical number of
earlier-zero constraints:

| Case | Variables | Constraints | Incidences | Placement-routing incidences | Expected unary additions |
| ---: | ---: | ---: | ---: | ---: | ---: |
| Parent | 63,385 | 161,632 | 618,978 | 242,663 | 0 |
| 0 | 63,385 | 161,634 | 618,980 | 242,663 | 2 |
| 1 | 63,385 | 161,635 | 618,981 | 242,663 | 3 |
| 2 | 63,385 | 161,636 | 618,982 | 242,663 | 4 |
| 3 | 63,385 | 161,635 | 618,981 | 242,663 | 3 |
| 4 | 63,385 | 161,636 | 618,982 | 242,663 | 4 |
| 5 | 63,385 | 161,637 | 618,983 | 242,663 | 5 |

The measured changes match the exact expected changes.

## Root Propagation in the Remaining Cases

The key-24 parent begins with 508 unresolved pipe route arcs and no pipe flow with a positive lower
bound. In cases 3 through 5, root propagation fixes three pipe arcs and three positive pipe flows:
the selected source continuation, the selected demand continuation, and one implied continuation.
Depending on the canonical demand case, 499 to 501 pipe arcs remain unresolved. All three cases
still begin native search at the same `pipe-bridge-59-rotation-90` Boolean as the parent.

The remaining source-fixed portfolio therefore removes fewer than two percent of the unresolved
pipe arcs and leaves the global pipe topology, item assignment, flow, bridge, five external pipe
terminals, and every other material network coupled.

## What Is and Is Not Proven

Proven:

- the six endpoint cases exactly cover the inherited parent leaf;
- `48 -> 32` is impossible in that leaf;
- `48 -> 64` is mandatory for every feasible solution in that leaf;
- fixing the source direction and any one of the three demand directions is not sufficient to find
  a witness within the concurrent five-second run.

Not proven:

- that the key-24 parent is feasible or infeasible;
- that any of cases 3 through 5 is feasible or infeasible;
- the minimal semantic cause of the `48 -> 32` root conflict;
- that endpoint propagation is the dominant global cost;
- that one timeout case is faster than another;
- that a leaf-specific fixed arc can be promoted directly to production logic.

## Next Exact Experiment

First run a source-only two-case control with no demand-continuation restriction. It must use the
same canonical source cases while leaving every demand candidate free.

- If the `48 -> 32` source-only case is root-proven infeasible, demand fixation is unnecessary to
  expose the contradiction and the source-side dead-end/connectivity hypothesis becomes stronger.
- If it remains `Unknown`, the three demand splits are collectively necessary for proof discovery
  even though their exact union proves the source region infeasible. That identifies a missing
  propagation opportunity across the demand disjunction.

The control should also add root-conflict provenance when Pumpkin exposes it: emptied domain and
semantic name, last constraint or propagator family, fixation assumption, and a material-capable
reachability/cut snapshot. Without this evidence, the experiment cannot attribute the root proof to
connectivity, flow conservation, item/topology, occupancy, capacity, or their combination.

After that control, continue inside the lowest unresolved child, case 3, where source `48 -> 64`
and demand `97 -> 113` are selected. Construct an item-specific separator cut between source and
demand for `item-liquid-xiranite-poly`.

For every directed cut arc `e`, define an exact material-carrying predicate:

```text
q_e <-> (flow(e) > 0 AND item(e) == item-liquid-xiranite-poly)
```

Then partition canonically by the first true `q_e`. This is an exact route-interior cut rather than
a selected path: every legal material route must cross the separator, while branching, merging,
cycles, bridges, crossings, and later cut arcs remain free.

The design must fail closed unless it proves:

1. the fixed source and demand lie on opposite sides of the cut;
2. the selected network has one fixed positive source and one fixed positive demand;
3. every directed grid arc crossing the cut is represented;
4. `q_e` is biconditionally channeled to both positive flow and the selected item;
5. the canonical cases include the exact complement for every earlier candidate;
6. a control parent contains the same `q_e` definitions so the only case difference is the
   canonical cut selection.

Separately, the root-conflict provenance should be instrumented before turning `48 -> 32 = 0` into
a general propagator. A general rule requires a semantic proof that applies beyond this one fixed
leaf; a repeated leaf-specific infeasibility result is not enough.

## Independent Review

Three independent post-run reviews examined soundness, measurement, and the next exact strategy.
All passed the run. They confirmed the six-case exact cover, the three duplicated root proofs, the
three unresolved complementary cases, authoritative/observation agreement, and every controlled
model delta.

The reviews also bounded the interpretation:

- the source implication is valid only inside the inherited fixed leaf;
- it may be preserved as a conditional leaf nogood but is not an unconditional production rule;
- the artifact proves a root contradiction but does not identify its detecting constraint or
  semantic cause;
- timeout counter differences are not speed rankings;
- harvesting this one inference alone would not advance the surviving five-second cliff;
- the source-only control should precede the item-specific separator experiment.

No unresolved soundness or artifact-consistency blocker remains.

## Improvements Preserved in the Exact Baseline

The run retains all exact improvements listed by the preceding endpoint-continuation report,
including shared transport layers, factored placement/port state, canonical occupancy, exact
dimension portfolios, possible-graph and local-continuation propagation, guarded item
intersection, exact residual/endpoint/boundary partitions, and root continuation certificates.
No layout, route, corridor, or shortest-path heuristic is introduced.

## Artifacts

- machine-readable report: `/tmp/aic-phase3-pipe-endpoint-continuation.uwWGuF/summary.json`;
- self-contained report: `/tmp/aic-phase3-pipe-endpoint-continuation.uwWGuF/summary.html`;
- per-case authoritative and observation wireframes:
  `/tmp/aic-phase3-pipe-endpoint-continuation.uwWGuF/case-*.html`.

The total chained diagnostic took 319,504 ms. The new authoritative wave took 5,694 ms, the
observation wave took 5,683 ms, and the endpoint experiment took 11,378 ms. The total includes
reconstruction of the full accepted parent chain and is not the cost of the six child solves alone.

## Verification

```text
cargo fmt --all -- --check
cargo test --workspace
cargo build --release -p aic-cli --bin aic-prior-terminal-pair
git diff --check
```

The existing endpoint implementation was unchanged from commit `45c4a8f`; this slice changes only
the selected network input and records the resulting exact experiment.
