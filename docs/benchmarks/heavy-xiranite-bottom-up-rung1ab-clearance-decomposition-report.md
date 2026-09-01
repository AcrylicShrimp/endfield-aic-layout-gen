# Heavy Xiranite Bottom-Up Rung 1A/1B Clearance Decomposition

## Question

The committed Rung 1 baseline introduced full directional rotation, compatible facility-port
selection, rotated connection geometry, and selected-connection clearance together. Its first
repeated five-second miss appeared at cumulative Phase 23 with 24 facilities, while Phase 24 with
25 facilities found a witness in about 0.4 seconds. This experiment asks whether the cliff and the
non-monotonic Phase 23/24 result originate before or after selected-port clearance is added.

## Exact Split

Both variants are fresh feasibility-only models. Neither receives a hint, incumbent, fixed
placement, fixed rotation, fixed port, routing state, flow state, or objective.

- **Rung 1A (`facility-port-geometry`)**: facility placement, full directional rotation,
  compatible port choice, exact `(rotation, port, local connection)` support, and an in-grid global
  connection coordinate.
- **Rung 1B (`facility-ports`)**: Rung 1A plus the requirement that every selected connection cell
  lies outside every non-owner facility footprint.

The dedicated semantic certificate records whether facility-endpoint clearance is present. A
controlled `4 x 2` fixture proves the split: Rung 1A accepts the only non-overlapping placement even
though its selected connection is covered by the blocker, while Rung 1B proves the same instance
infeasible.

## Phase 23/24 Result

All runs used the release binary, the Heavy Xiranite minimum-rate workload, a loose `50 x 50`
request ceiling, and a five-second first-witness budget.

| Phase | Facilities | Endpoints | Variant | Variables | Constraints | Outcome | Search | Decisions | Conflicts | Propagations |
|---:|---:|---:|---|---:|---:|---|---:|---:|---:|---:|
| 23 | 24 | 71 | Rung 1A | 2,840 | 4,069 | feasible 4/4 | 50-52 ms | 4,486 | 122 | 187,078 |
| 23 | 24 | 71 | Rung 1B | 10,444 | 13,574 | unknown 4/4 | 5,000 ms | about 160k | about 10k | about 23M |
| 24 | 25 | 75 | Rung 1A | 3,012 | 4,301 | feasible 4/4 | 97-101 ms | 9,156 | 193 | 351,187 |
| 24 | 25 | 75 | Rung 1B | 11,348 | 14,721 | feasible 4/4 | 370-403 ms | 15,111 | 907 | 1,438,691 |

At Phase 23, clearance alone adds 7,604 Boolean variables and 9,505 constraints. The delta is
exactly:

```text
7,604 reified directional separation inequalities
1,901 guarded four-way separation clauses
```

Rung 1A reaches its first witness in about one ninetieth of the time exhausted by Rung 1B. This
isolates the Phase 23 cliff to the clearance block or its interaction with the unchanged placement,
rotation, port, and coordinate decisions. Directional rotation by itself is not the Phase 23
blocker.

The Phase 23/24 inversion also disappears before clearance: Rung 1A grows from 55 ms to 85 ms.
Therefore the earlier inversion is generated inside the dense clearance/search interaction, not by
an intrinsically easier 25-facility semantic problem.

## Representation Cost

The current Rung 1B encoding considers every tuple:

```text
(selected endpoint, non-owner facility, facility geometry class)
```

and creates four reified inequalities: the point is left, right, above, or below the rectangle. A
guarded clause requires at least one separation for the selected facility geometry. This is exact,
but its auxiliary state grows with endpoints multiplied by facilities and geometry classes.

The recorder calls Pumpkin's full `NegatableConstraint::reify`, not `implied_by`. Pumpkin therefore
posts both directions and each Boolean is equivalent to its geometric inequality. There is no
extra family of Boolean assignments for one fixed geometry. The cost is still real: these derived
truth values become tens of thousands of solver variables and generic propagators, and the default
brancher may select them before the smaller set of placement, rotation, and port decisions has
determined them.

At the full 59-facility Phase 40 graph:

| Metric | Rung 1A | Rung 1B | Clearance delta |
|---|---:|---:|---:|
| Variables | 12,402 | 62,738 | +50,336 |
| Constraints | 17,215 | 80,135 | +62,920 |
| Reified clearance inequalities | 0 | 50,336 | +50,336 |
| Guarded clearance clauses | 0 | 12,584 | +12,584 |

The model is separately materializing one derived Boolean for each directional fact even though the
semantic relation is only that one point must not lie inside one variable rectangle.

## Rung 1A Growth

Removing clearance does not make the full graph trivial. The first repeated five-second miss moves
from Phase 23 to Phase 39.

| Phase | Facilities | Endpoints | Outcome wave | Search evidence |
|---:|---:|---:|---|---|
| 38 | 53 | 177 | feasible 4/4 | 4.00-4.10 s, 179,668 decisions |
| 39 | 56 | 187 | unknown 4/4 | 5.00 s, about 194k-199k decisions |
| 39 | 56 | 187 | feasible 30 s confirmation | first witness at 7.347 s, 397,236 decisions |
| 40 | 59 | 197 | unknown in 5 s | no five-second witness |

This is a second, later first-witness cost increase inside placement, rotation, port support, and
coordinate channeling. It must be decomposed separately after the clearance representation is
replaced or evaluated, because Rung 1B currently fails much earlier and masks it.

## Conclusion

The measured first blocker is the current selected-endpoint clearance representation. This is not
evidence that the game rule should be removed: the rule distinguishes a usable port from one buried
under another facility. It is evidence that the exact rule is represented with too many auxiliary
direction Booleans and weak generic search exposure.

The next semantics-preserving experiment should replace the four reified directions with one
specialized point-versus-variable-rectangle clearance relation per endpoint/facility pair, retaining
all placement, rotation, port, and coordinate choices. It must provide sound explanations and be
compared against both Rung 1A and the current Rung 1B baseline. Dense per-cell occupancy is a
separate candidate, but the loose `50 x 50` ceiling makes it a poor default unless measurement
shows otherwise.

After the clearance experiment, rerun cumulative growth. If the first repeated miss remains near
Phase 39, decompose Rung 1A into endpoint support, coordinate channeling, and default branching as a
new independent cliff.

## Independent Review Resolution

Independent soundness and optimization reviews found no blocker in the Rung 1A/1B split or its
witness validators. The soundness review confirmed the half-open rectangle inequalities, owner
omission invariant, and exact controlled infeasibility fixture.

One initial optimization-review claim said that Pumpkin's reification was only one-way and created
multiple Boolean assignments for one geometry. Local source verification rejected that claim:
`RecordedModel::post_reified_less_than_or_equals` calls `NegatableConstraint::reify`, which posts
both the constraint under the literal and its negation under the complemented literal. The reviewer
then retracted the claim. The accepted performance hypothesis is representation size and branch
indirection, not duplicate solutions.

The lower-risk first replacement is one exact `connection_x/y` versus variable-rectangle
propagator per endpoint/non-owner facility pair. A later offset-aware variant may directly connect
owner origin and local port key to the target rectangle for stronger port back-propagation, but it
has a larger proof surface. The replacement must pass exhaustive small-grid equivalence, all-side
and all-rotation fixtures, domain-hole tests, root bidirectional probes, independent explanation
review, unchanged witness validation, and equal-budget release benchmarks.

## Artifacts

- `heavy-xiranite-bottom-up-rung1ab/geometry-phase23/`
- `heavy-xiranite-bottom-up-rung1ab/clearance-phase23/`
- `heavy-xiranite-bottom-up-rung1ab/geometry-phase24/`
- `heavy-xiranite-bottom-up-rung1ab/clearance-phase24/`
- `heavy-xiranite-bottom-up-rung1ab/geometry-adjacent-repeat/`
- `heavy-xiranite-bottom-up-rung1ab/geometry-growth/`
- `heavy-xiranite-bottom-up-rung1ab/geometry-repeat/`
