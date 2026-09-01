# Heavy Xiranite Bottom-Up Rotation Growth

## Result

The complete exact directional-rotation partition that crosses the Phase 30 cliff also crosses
Phase 31 after adding one additional rotation axis. It does not cross Phase 32, even after adding
each of the three newly introduced facilities as a third axis or after extending the best known
Phase 31 rotation assignment to four 30-second conditioned children.

The bottom-up rung comparison localizes the Phase 32 first-witness cliff to endpoint clearance.
Facility placement, directional rotation, port choice, and connection-cell geometry without
clearance produce a validated witness in 306 milliseconds. Adding the exact rule that every
selected connection cell must lie outside every non-owning facility footprint prevents both the
generic reified formulation and the custom propagated formulation from finding a witness within
five seconds.

This is a feasibility diagnosis. No routing or optimization objective is present in these rungs.

## Exact Growth Experiment

Every unconditioned partition enumerates the complete Cartesian product of the selected
facilities' fitting directional-rotation domains. The children are pairwise disjoint and their
union is the original parent. A feasible child therefore proves parent feasibility. Unknown
children do not prove infeasibility.

| Phase | Facilities | Terminals | Exact partition | Result | First feasible wall | Full wall |
|---:|---:|---:|---:|---|---:|---:|
| 30 | 33 | 105 | one facility, 4 children | 2 feasible, 2 unknown | 0.4-0.8 s child search | about 11.3 s sequential evidence |
| 31 | 36 | 115 | retained Phase 30 axis, 4 children | all unknown | - | about 5.0 s |
| 31 | 36 | 115 | retained axis plus one new facility, 16 children | 1 feasible, 15 unknown | 13,393 ms | 20,188 ms |
| 32 | 39 | 125 | retained two axes, 16 children | all unknown | - | 20,211 ms |
| 32 | 39 | 125 | retained two axes plus new facility 0, 64 children | all unknown | - | about 80.8 s |
| 32 | 39 | 125 | retained two axes plus new facility 1, 64 children | all unknown | - | about 80.8 s |
| 32 | 39 | 125 | retained two axes plus new facility 2, 64 children | all unknown | - | about 80.8 s |

The Phase 31 witness fixes both selected seed collectors to 180 degrees. Its child search takes
3,249 milliseconds, 160,445 branch decisions, 3,768 conflicts, and 25,443,000 solver
propagations. The 13,393-millisecond first-feasible wall time includes parallel scheduling and
earlier unknown children.

The Phase 32 conditioned diagnostic retains those two 180-degree decisions and partitions one
new facility into four children. Each child receives 30 seconds. All four remain unknown after
approximately 756,000-792,000 decisions and 189-193 million propagations. This is an exact
partition only of the conditioned subproblem. It neither proves the original Phase 32 parent
infeasible nor establishes that the Phase 31 rotations belong to every Phase 32 witness.

## Phase 32 Rung Decomposition

| Rung | Clearance encoding | Outcome | Search | Variables | Constraints | Decisions | Conflicts | Propagations |
|---|---|---|---:|---:|---:|---:|---:|---:|
| `facility-port-geometry` | absent | feasible | 306 ms | 6,450 | 9,085 | 25,200 | 431 | 1,324,505 |
| `facility-ports` | generic reified inequalities | unknown | 5,001 ms | 28,354 | 36,465 | 126,230 | 4,394 | 12,999,716 |
| `facility-ports-propagated` | custom point-rectangle propagator | unknown | 5,000 ms | 6,450 | 13,835 | 234,185 | 11,821 | 51,802,190 |

The custom propagator removes 21,904 variables (77.2%) and 22,630 constraints (62.1%) relative
to the generic reified model. That static reduction does not remove the first-witness cliff.

Its five-second Phase 32 runtime counters are:

| Counter | Value |
|---|---:|
| endpoint-to-non-owner-facility relations | 4,750 |
| propagator executions | 39,149,633 |
| notifications | 49,528,209 |
| coordinate notifications | 46,624,873 |
| orientation notifications | 2,903,336 |
| orientation checks | 41,724,845 |
| rejected orientations | 1,523 |
| forced-separation detections | 495,065 |
| bound updates | 34,445 |
| clearance conflicts | 1,295 |

Most activity is therefore not model construction. It is repeated dynamic work after coordinate
events: about 8,242 executions per posted relation and only about one bound update per 1,137
executions. These ratios do not by themselves prove that the executions are redundant; backtrack
re-entry and conflict discovery may require repeated checks. They do establish the next diagnostic
target.

## Finding

Rotation partitioning is a sufficient exact cut at Phase 30 and Phase 31, but it is not a general
explanation of the next cliff. Phase 32 remains hard after all tested complete rotation products.
The first newly difficult semantic rule is the exact coupling:

```text
selected endpoint connection cell
must be outside
every non-owning facility's rotated footprint
```

The current implementation posts one custom propagator per endpoint/facility relation. Every
relation watches the endpoint coordinates, target facility coordinates, and target orientation
selectors. The aggregate counters show that coordinate notifications dominate its runtime.

## Next Diagnostic

Instrument relation-level work without changing pruning semantics:

1. classify executions by triggering event family;
2. count executions that inspect no changed bounds since the relation's previous fixpoint;
3. count executions that reject an orientation, update a bound, or report a conflict;
4. aggregate executions and useful inferences by endpoint, target facility, and relation;
5. report hot-set concentration and useful-inference yield at Phases 29, 31, and 32.

Only after that evidence should a grouped or indexed exact propagator be selected. A grouped
implementation may share immutable relation indexes and coalesce wakeups for the same semantic
proof rule, but it must preserve every legal placement, rotation, and port choice and must produce
the same sound eager relation-specific reasons for every inference.

## Artifacts

Raw JSON and HTML evidence is under
`docs/benchmarks/heavy-xiranite-bottom-up-rotation-growth/`.
