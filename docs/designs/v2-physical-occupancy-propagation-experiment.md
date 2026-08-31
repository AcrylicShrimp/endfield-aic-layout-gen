# V2 Physical Occupancy Propagation Experiment

## Status

Proposed exact-formulation diagnostic. This document records the current encoding and the
experiment plan only. No solver behavior changes are approved by this checkpoint.

## Question

How strongly does one physical occupancy decision propagate between facility placement and the
shared belt and pipe layers at Pumpkin's root fixed point, and can one canonical physical occupancy
channel strengthen that propagation without changing the legal layout set?

The primary metric is cross-domain pruning caused by one occupancy fact. Raw model-size reduction
is secondary.

## Game Semantics

For every physical cell `c`:

```text
facility(c) + belt(c) <= 1
facility(c) + pipe(c) <= 1
```

`belt(c)` and `pipe(c)` may both be one because they occupy different height layers. Internal route
cells, logistics components on those routes, and external connector cells all contribute to the
corresponding transport-layer occupancy.

## Current Encoding

There is no canonical `facility_occupied[c]` variable. The value currently named
`facility_occupancy[c]` is a vector containing every placement-candidate Boolean whose footprint
covers cell `c`.

For every facility instance, the formulation enumerates every legal `(rotation, x, y)` tuple and
posts:

```text
sum(candidate[i]) = 1
```

For every internal transport layer and cell, it posts:

```text
sum(facility candidates covering c) + internal_route_cell[layer,c] <= 1
```

The external-connector collision pass separately posts, once per transport kind and cell:

```text
sum(facility candidates covering c)
  + internal_route_cell[layer,c]
  + sum(external_connector_cell[connector,c]) <= 1
```

Consequently, a layer with an internal route currently receives both the internal-only collision
row and the later combined collision row. Belt and pipe are kept independent because no collision
row contains variables from both transport kinds.

`internal_route_cell[layer,c]` is an exact presence indicator over incoming and outgoing route arms.
Each external connector cell is an exact presence indicator over that connector's geometry
contributors. Selecting any contributing arm or connector geometry therefore propagates its cell
indicator to one before collision propagation.

The v2 endpoint model also has one integer placement-choice index, but it is channelled to the
candidate Booleans only by the wide weighted equality:

```text
placement_choice = sum(i * candidate[i])
```

There are still no independent x, y, or rotation domains.

## Expected Current Root Propagation

The following claims come directly from the posted linear constraints and must be confirmed by the
diagnostic rather than assumed as performance evidence.

### Transport to placement

If a belt route cell, pipe route cell, or external connector cell is fixed to one, its collision
row becomes:

```text
sum(covering candidates) <= 0
```

The linear propagator can immediately fix every covering candidate Boolean to zero. This direction
should be strong and should happen at root before search.

### Exact placement to transport

If one placement-candidate Boolean is fixed to one, every belt and pipe occupancy contributor in
its footprint is fixed to zero by the per-layer collision rows. This direction should also be
strong.

### Partial placement domain to transport

If multiple placement candidates remain and every remaining candidate covers cell `c`, the model
logically implies that both transport layers are unavailable at `c`. The current constraints do not
share a Boolean representing that derived fact. The exactly-one row knows that one remaining
candidate must be selected, while each collision row sees each candidate with an individual lower
bound of zero. Bounds propagation need not combine the two rows to infer that their sum is one.

The expected weak case is therefore:

```text
all remaining placement candidates cover c
transport_layer[c] still has domain {0,1}
```

A fixed combined placement index may also channel asymmetrically because the index equality is a
weighted bounds constraint rather than an inverse element or tuple-table channel.

## Root-Propagation Probe

Add a research-only CLI command and report schema. The probe must construct a small model with the
same candidate generator, exactly-one relation, presence indicators, and collision posting helpers
as the authoritative v2 formulation. It must not run the layout optimizer or publish probe geometry
as a valid layout.

Use one real 5 by 5 Heavy Xiranite facility under a 12 by 12 request ceiling. This gives 256 legal
placement candidates. Use a deterministic interior target cell whose footprint-membership set is
recorded in the report. For example, cell `(6,6)` is covered by 100 candidates in the square 5 by 5
case, while a corner cell provides a low-degree control.

Every case gets a fresh solver. Snapshot all relevant domains after the base model reaches its root
fixed point, post exactly one diagnostic-only restriction, let Pumpkin reach the next root fixed
point, and snapshot again.

### Baseline cases

| Case | Root restriction | Required observation |
| --- | --- | --- |
| `none` | none | Initial candidate and transport domains |
| `belt-used` | target belt cell equals one | Covering candidates removed; target pipe remains legal |
| `pipe-used` | target pipe cell equals one | Covering candidates removed; target belt remains legal |
| `exact-placement` | one placement candidate equals one | Belt and pipe cells in the complete footprint become zero |
| `same-footprint-domain` | keep the four rotations at one x/y origin; remove all other candidates | Whether both transport layers become zero for all 25 logically mandatory footprint cells |
| `non-covering-control` | keep a placement subset that does not force occupancy of the target cell | Target belt and pipe remain legal |

The fixed restrictions are diagnostic assumptions, not layout heuristics or production solver
behavior. Every baseline and reformulated case must use the same legal-state question.

## Required Metrics

For every variable family, record domain cardinalities before and after root propagation, not only
lower and upper bounds. At minimum report:

- placement candidates with value one still supported;
- placement candidates newly fixed false, split into target-covering and non-covering candidates;
- combined placement-index values still supported;
- derived distinct x, y, and rotation values among supported candidates;
- belt and pipe cells fixed true, fixed false, or still free;
- transport states removed inside and outside the selected facility footprint;
- number and total arity of collision rows incident to the forced fact;
- collision rows containing at least one changed domain and rows becoming fully decided;
- total related-domain removals and root fixed-point elapsed time;
- root inconsistency, if any.

Pumpkin 0.5 publicly exposes root literal values, lower and upper bounds, and exact domain membership.
It does not expose structured per-propagator call counts through the public Solver API. Constraint
activity in this experiment therefore means incident collision rows and their observed domain
changes. Do not claim an internal propagator-call count unless a separate stable measurement path is
found.

The JSON report must include each target cell, the complete pre/post counts, the analytically
expected number of covering candidates, and an explicit propagation-strength verdict. A compact
self-contained HTML table should make asymmetric pruning visible; it is not a layout witness.

## Exact Reformulation Candidate

If the baseline confirms the partial-domain weakness, compare it against one canonical shared
physical occupancy encoding.

### Placement state

Make the placement-choice integer authoritative. For each facility instance `f` and physical cell
`c`, derive a Boolean through an arc-consistent constant element relation:

```text
instance_occupied[f,c] = footprint_mask[f,c][placement_choice[f]]
```

Then define the canonical facility state:

```text
facility_occupied[c] = sum(instance_occupied[f,c])
```

Because `facility_occupied[c]` is Boolean, this equality also enforces facility non-overlap. The
candidate metadata remains the complete legal tuple table, but footprint collision no longer
depends on one Boolean per tuple or on the weak weighted candidate-index channel.

### Transport-layer state

Define one canonical Boolean per layer and physical cell:

```text
belt_occupied[c]
  = internal_belt_route_cell[c] + sum(belt_external_connector_cell[connector,c])

pipe_occupied[c]
  = internal_pipe_route_cell[c] + sum(pipe_external_connector_cell[connector,c])
```

The Boolean equality preserves same-layer exclusivity and exact presence. It does not relate belt
to pipe.

Post only the physical collision channel:

```text
facility_occupied[c] + belt_occupied[c] <= 1
facility_occupied[c] + pipe_occupied[c] <= 1
```

This representation should propagate in both directions:

- a used belt or pipe cell sets `facility_occupied[c]` to zero, which removes every placement-choice
  value whose footprint covers the cell;
- a placement domain whose every supported value covers a cell makes
  `instance_occupied[f,c] = 1`, then excludes both transport layers;
- an exact placement excludes both layers over its entire footprint;
- belt and pipe remain independently usable at the same coordinate.

The constant element propagator in Pumpkin 0.5 is arc-consistent, so it is an appropriate exact
channel for the complete footprint-mask table. The reformulation must remove superseded candidate
collision and duplicate internal-only rows rather than retaining parallel compatibility paths.

## Equivalence Requirements

Before any performance claim, exhaustively compare the baseline and canonical encodings on a small
grid. For every legal placement and target-cell belt/pipe assignment, both encodings must accept the
same states. Explicitly test:

- facility versus belt exclusion;
- facility versus pipe exclusion;
- simultaneous belt and pipe occupancy without a facility;
- exact placement footprint exclusion;
- rotation-dependent footprints;
- multiple facilities and facility non-overlap;
- internal route and external connector contributors on the same layer.

The reformulation is rejected if it removes a legal state, permits a collision, couples belt to
pipe, or changes placement/rotation/port/routing decisions.

## Comparison Matrix

Run the root-propagation probe for both encodings first. The reformulation passes the propagation
gate only if it preserves the legal states and strictly improves at least the
`same-footprint-domain` case without weakening `belt-used`, `pipe-used`, or `exact-placement`.

Only after that gate, run release-mode full-model comparisons:

1. Heavy Xiranite phase 1 at 12 by 12, where the current first incumbent is 1,179 ms;
2. Heavy Xiranite phase 1 at 13 by 13, where the current model has no incumbent in 5,000 ms;
3. cumulative phase 2 and phase 3 at 12 by 12 if the first two cases retain validation and improve
   propagation or incumbent discovery.

Give every full solve the same 5,000 ms budget. Record model construction, first incumbent, final
objective, proof, validation, variable/constraint/term families, placement-routing incidences, peak
RSS, and the root-propagation metrics. A timeout remains `unknown`.

## Falsifiable Outcomes

- If the current model already propagates the partial placement domain into both transport layers,
  the shared occupancy hypothesis is rejected before reformulation.
- If the canonical model strengthens the probe but not the 13 by 13 phase-1 cliff, physical
  occupancy is not the dominant full-model blocker; inspect external connector geometry next.
- If it restores the 13 by 13 incumbent without changing semantics or objective, repeat cumulative
  growth before accepting it as the new exact baseline.
- If model size falls but root pruning does not improve, do not accept the change on size alone.

## Decision Boundary

This checkpoint ends with the current-encoding analysis and experiment contract. Do not implement
the probe or canonical occupancy reformulation until the user reviews this plan. After approval,
commit the diagnostic probe and its baseline measurements before implementing the alternative
encoding.
