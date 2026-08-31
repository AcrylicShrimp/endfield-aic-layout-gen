# Phase 3 Representative Prior-Input Port Pair

## Status

Accepted exact diagnostic contract following the representative single-port controls.

## Question

Does jointly fixing both remaining old-source belt input ports cross the five-second first-feasible
cliff inside the same predeclared Phase 3 source leaf?

## Parent State

Rerun the complete representative prior-input control stage and retain its source leaf index 0,
nine inherited terminal assignments, prior reference, fixed prior-overlap placements, fixed used
dimensions, and introduced-facility state.

Stop interpretation if the control stage contains an invalid witness. A validated parent or control
witness already proves the representative leaf feasible and must remain visible in the aggregate.

## Proof-Driven Residual Domains

For each controlled terminal, require one complete single-port suite containing every declared port
exactly once. Exclude a port from the pair domain only when its case outcome is
`ProvenInfeasible` while the other terminal remained free. Record the suite index, terminal, port,
and proof outcome that justifies every exclusion.

Do not exclude a port because its displayed connection position is absent. Unknown and validated
feasible values remain in the residual domain. An invalid witness blocks the experiment.

The measured control stage proves `input-belt-4` impossible for either terminal, leaving the exact
residual domains `input-belt-0` through `input-belt-3`.

## Exact Pair Portfolio

Enumerate the complete ordered Cartesian product of the two residual domains. Include assignments
where both logical demands choose the same physical facility port; no all-different game rule has
been accepted.

Each child adds exactly two terminal-port equalities to the inherited nine assignments. It leaves
all other facility ports, external terminals, route cells, route arcs, material assignments, flow,
topology, capacity, occupancy, logistics components, bridges, and objective variables under solver
control.

For the current residual this produces 16 cases. Combined with the already-proven empty row and
column from the control stage, these 16 cases exactly cover the representative source leaf.

## Measurements

Record:

- complete control-stage provenance and proof exclusions;
- both residual terminal domains;
- all ordered pair assignments and optional connection positions;
- outcome, construction time, search time, and first-incumbent time;
- decisions, backtracks, conflicts, learned clauses, propagations, and restarts;
- variables, constraints, incidences, and placement-routing incidences;
- worker count, case budget, pair-wave wall time, and total hierarchy wall time;
- validated witness, complete representative infeasibility, unknown, and invalid status.

Emit machine-readable JSON, one self-contained summary HTML, and one standalone HTML artifact for
every pair case regardless of outcome.

## Interpretation

- Any validated pair witness proves the representative leaf feasible, not optimal.
- All residual pairs proven infeasible, together with the control proofs, prove the representative
  leaf infeasible.
- Mixed proven and unknown results identify exact endpoint-pair subregions for further refinement.
- All residual pairs unknown means complete old-source facility endpoint fixation did not cross the
  five-second cliff in this representative leaf.
- No conclusion generalizes to the other 39 source leaves or global Phase 3.

## Next Observation

Predeclare pair index 0 as the first pair before examining outcomes. If it remains unknown, use it
for the read-only snapshot. If it is proven infeasible at root, select the lowest-index unknown pair
as a deterministic surviving sample after the pair experiment and record that fallback rule and
selection explicitly. This selection may guide observation only; it cannot support a proof about
the other unknown pairs.

Take the snapshot at the first branch after root propagation. First verify that the inherited and
pair-fixed facility endpoint geometry domains are singleton. Then measure unresolved endpoint
options, material-capable route arcs and arms, item supports, flow bounds, terminal routing options,
and external-terminal domains. The observer must not post a constraint, prune a value, or change
branch order.
