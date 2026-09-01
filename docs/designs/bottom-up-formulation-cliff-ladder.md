# Bottom-Up Formulation Cliff Ladder

## Status

Accepted research experiment. This document defines a diagnostic harness, not a production solve
orchestration architecture.

## Purpose

The current complete joint model combines placement, directional port selection, external
terminals, two transport layers, flow, topology components, collisions, and crossings before search
begins. Top-down fixation experiments can identify residual freedom, but each result remains coupled
to every other semantic block.

The bottom-up ladder builds fresh cumulative models. Each rung adds exactly one semantic block to
the previous contract. The first transition from a tractable model to a first-witness or
optimization cliff identifies the newly added block and its coupling to the earlier formulation.

The ladder does not:

- fix a witness from an earlier rung;
- pass an incumbent or hint between rungs in the baseline experiment;
- split one production solve into several authoritative solves;
- claim that an incomplete rung is a legal AIC blueprint;
- replace the final exact joint model.

## Controlled Input

Every rung receives the same prepared cumulative production graph, validated game catalogs, hard
`max_width` and `max_height` ceilings, time budget, stable entity order, and solver build. The first
baseline workload is Heavy Xiranite minimum-rate cumulative Phase 3.

The initial comparison uses a `16 x 16` coordinate canvas and hard ceiling because that is the
currently observed Phase 3 cliff case. It is a diagnostic request bound, not a game constant or
required used footprint. Partial rungs must not require their partial geometry to occupy exactly
`16 x 16`: omitted future transport may legitimately extend the final footprint.

## Cumulative Rungs

### Rung 0: Facility Geometry

Solver decisions:

- facility placement candidate, including origin and allowed rotation, for every facility;
- canonical physical facility occupancy;
- facility-only width and height derived from facility occupancy for observation only.

Hard constraints:

- exactly one placement per facility;
- every selected footprint remains inside the request ceiling;
- facility footprints do not overlap;
- canonical translation may remove whole-layout translation symmetry only after a checked orbit
  proof for the partial constraints. Translating the complete partial assignment toward the origin
  must preserve every partial constraint and must not be confused with fixing a full-layout
  bounding box.

No port, boundary-terminal, transport, item, flow, or logistics-component variable may be present.

### Rung 1: Facility Ports

Adds one compatible facility-port choice and its rotated physical geometry for every facility-owned
logical endpoint. Port direction, transport kind, input/output direction, placement, rotation, and
physical port geometry remain linked by the exact endpoint-support relation.

External boundary-terminal coordinates are not added. An external logical endpoint contributes only
its facility-owned counterpart at this rung. No route, flow, item-on-grid, or topology variable may
be present.

There is no invented port-distance objective. Port assignment has no independent game-quality
objective before routing exists. The placement geometry objective remains unchanged.

### Rung 2: Pipe Routing

Adds the complete pipe semantic block:

- pipe external boundary terminals;
- the shared pipe layer;
- pipe route-cell, directed-arc, arm-item, and integer-flow state;
- pipe terminal presence, item assignment, conservation, capacity, collision, connectivity, turns,
  splitters, convergers, and same-layer pipe bridges;
- facility-versus-pipe physical occupancy coupling.

All placement and facility-port decisions remain solver decisions. Belt facility-port choices from
Rung 1 remain present, but no belt grid or belt boundary-terminal variable is added.

Pipe bridges remain enabled. This keeps Rung 2 a relaxation of the full model: every full solution
projects to some legal Rung 2 assignment.

### Rung 3: Belt Routing Without Same-Layer Bridges

Adds the complete belt block but fixes every belt bridge-selection Boolean to zero. Pipe bridges
remain enabled. Belt and pipe remain separate transport layers and may occupy the same world cell.
Splitters and convergers remain available.

This rung is a restricted diagnostic subset. Its infeasibility or timeout does not imply anything
about Rung 4 feasibility.

### Rung 4: Complete Joint Model

Removes the belt bridge-selection restriction. This is the currently approved exact joint
formulation:
placement, rotation, directional ports, belt routing, pipe routing, flow, topology, collision, and
same-layer crossing are solved together.

## Output Contract

The primary ladder is feasibility-only. It asks where the first complete witness search breaks before
optimization is allowed to obscure the result. Partial-rung objective values are not compared:
omitted geometry makes them different optimization problems. A facility-only area is a valid lower
bound on full area, but a zero belt-tile count in a pipe-only model is not a full objective incumbent.

After a rung is robustly feasible, a separate optimization observation may test its own objective
proof cost. Only Rungs 3 and 4 contain all physical geometry and share the complete production
objective vector.

Each fresh solve emits one `FormulationRungReport` containing:

- schema version, workload identity, phase, rung, ceiling, and budget;
- semantic-block certificate and cumulative-block list;
- construction time and search time;
- first incumbent time and incumbent count;
- termination reason and rung-specific witness-validation status;
- objective stage, incumbent, best bound, proof status, and final objective vector only in a
  separately labelled optimization observation where applicable;
- branch decisions, backtracks, conflicts, learned clauses, solver propagations, and custom
  propagator counters;
- complete variable-domain, constraint-family, factor-incidence, coupling, and estimated-memory
  metrics;
- root-domain snapshot coverage for every variable family present in that rung;
- an explicitly partial rung-specific witness containing only semantics enabled by the rung;
- automatic HTML evidence for success, timeout, invalid witness, and proof outcomes.

The aggregate `FormulationLadderReport` contains adjacent-rung model deltas and the first classified
cliff. It must preserve raw results even when interpretation is blocked.

## Build And Interpretation Gates

Every rung must fail closed when:

- its certificate omits or adds a semantic family outside the declared cumulative contract;
- repeated builds of the same rung are not structurally identical;
- an adjacent-rung delta contains an undeclared variable, constraint, or factor-incidence family;
- a successful search fails rung-specific witness validation;
- repeated identical models yield contradictory feasible and proven-infeasible evidence;
- root-snapshot coverage omits a declared variable family.

Lower-rung witnesses must not reuse `IntegratedLayoutReport.success`, because they are deliberately
not complete layouts. Each rung has a dedicated witness DTO and validator. Every build certificate
also proves that there are no hints, prior-solution fixations, replay clauses, transferred learned
clauses, or dynamically imported bounds.

A cliff is reported only when all relevant build and validation gates pass. Initial classification:

- `build-cliff`: construction exceeds its budget or resource cap;
- `first-witness-cliff`: the prior rung finds a validated witness and the new rung finds none;
- `proof-cliff`: both find witnesses, but the new rung cannot prove the comparable objective stage;
- `quality-cliff`: the new rung's objective progress is materially worse at equal checkpoints;
- `no-cliff`: both rungs finish within the declared budget;
- `inconclusive`: censored or invalid evidence prevents the comparison.

These labels identify where to investigate; they do not by themselves identify the cause.

The authoritative wave uses four fresh release-mode processes per rung, five seconds of search per
process, and a counterbalanced forward/reverse order. A separate root-only wave stops before the
first branch. If the first robust cliff has four `Unknown` results and no witness, one predeclared
thirty-second run confirms that the result is not a narrow five-second cutoff artifact.

The adjacent-rung certificate records canonical hashes for stable variable descriptors
`(name, family, domain)` and normalized constraint descriptors. It reports preserved, added,
removed, and replaced descriptors rather than assuming every rung is a syntactic superset.

## Decision Rule After The First Cliff

Inspect only the newly added semantic block and its coupling boundary first:

1. compare variable-domain volume and exact model-family delta;
2. compare root-domain pruning and first branch family;
3. determine whether the cost is model volume, weak bidirectional channeling, symmetry, generic
   branching, or repeated semantic reasoning;
4. prefer an exact reformulation or solver-native constraint when it strengthens propagation;
5. add a custom propagator only when a specific sound semantic inference is missing or repeatedly
   recomputed;
6. independently review proof soundness, implementation risk, and alternative formulations before
   committing a propagator.

After an improvement, rerun the ladder from Rung 0 through the improved cliff and then continue to
the next rung.

## Implementation Cutover

Do not add another rung variant to the current `solve_with_endpoint_encoding` mode matrix. The
existing shared-layer orchestration already combines endpoint encoding, research restrictions,
collectors, connectivity propagation, search, extraction, and validation in one large function.

Implement the ladder through staged fresh-model construction:

- `builder`: owns the staged build and typed placement, endpoint, routing, and complete artifacts;
- `placement`: owns placement candidates and canonical facility occupancy;
- `endpoints`: separates facility endpoint support from external and transport-specific terminal
  materialization;
- `routing`: builds exactly one requested shared transport layer;
- `geometry`: builds occupancy and used geometry only for active layers;
- `search` and `witness`: run a fresh solve and extract or validate a rung-specific witness.

The first implementation slice is behavior-neutral. It introduces typed stage artifacts and build
snapshots around the existing complete-model build while preserving creation and posting order.
Existing public solve wrappers must continue to build, search, extract, and validate the same full
model. Structural equivalence requires identical ordered variable descriptors, aggregate
constraint-family and factor-incidence metrics, formulation ID, and representative outcomes.

Before adjacent rung deltas become authoritative, extend model recording to retain normalized
constraint descriptors. Aggregate counts alone cannot detect offsetting unintended changes.

Only after the complete builder passes its equivalence gate should Rung 0 receive a partial witness
DTO. Facility endpoint construction must then be separated before Rung 1, and active-layer occupancy
must be generalized before Rung 2. The current full-terminal builder, hard-coded two-layer occupancy,
full-layout extractor, and all-layer crossing restriction are not valid shortcuts for partial rungs.

## Reviewed Soundness Notes

Independent soundness, experiment-design, and architecture reviews established the following
constraints on implementation:

- partial rungs use the request dimensions as ceilings, never as exact partial footprints;
- Rungs 0 through 2 are relaxations of the full problem; their infeasibility can refute the full
  problem, but their witnesses are only projections;
- Rung 2 keeps pipe bridges enabled;
- Rung 3 is a complete-layout subset that restricts belt bridges only;
- Rung 4 is the unrestricted full formulation;
- a solved-to-`Unknown` transition implicates the new block and its coupling, not one unique cause;
- any proposed custom propagator follows a same-semantics sub-ladder and independent proof review;
- port exclusivity, item filtering, or port-local capacity must not be invented while those game
  semantics remain unresolved in the shared mathematical model.
