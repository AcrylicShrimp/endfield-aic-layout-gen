# Bottom-Up Solver-Problem Cliff Ladder

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
optimization cliff identifies the newly added block and its coupling to the earlier problem.

The first question after a cliff is whether the newly added solver problem is the right problem at
all. A slow model may ask the solver to decide a relation that game semantics already determine,
prove a property that the output contract does not require, or distinguish states that are
observationally equivalent. Those are contract or problem-decomposition defects, not formulation
defects. Formulation quality is investigated only after the semantic decision set is justified.

The ladder does not:

- fix a witness from an earlier rung;
- pass an incumbent or hint between rungs in the baseline experiment;
- split one production solve into several authoritative solves;
- claim that an incomplete rung is a legal AIC blueprint;
- replace the final exact joint model.

Every rung reports the layered search profile defined in
[`search-space-evaluation.md`](search-space-evaluation.md). Semantic candidate volume, declared
solver-domain volume, and observed search work remain separate because they measure different
objects.

## Controlled Input

Every rung receives the same prepared cumulative production graph, validated game catalogs, hard
`max_width` and `max_height` ceilings, time budget, stable entity order, and solver build. The first
baseline workload is Heavy Xiranite minimum-rate cumulative Phase 3.

Every benchmark declares its own coordinate ceiling. The first full-growth ladder uses a loose
`50 x 50` diagnostic ceiling so facility and port feasibility can be observed without conflating
the experiment with a compact-footprint proof. This is neither a game constant nor a required used
footprint. Partial rungs report their actual used geometry independently of unused canvas capacity.

## Cumulative Rungs

### Rung 0: Facility Geometry

Solver decisions:

- facility origin and occupied footprint orientation for every facility;
- facility-only used width and height computed from the validated witness for observation only.

Hard constraints:

- exactly one placement per facility;
- every selected footprint remains inside the request ceiling;
- facility footprints do not overlap;
- canonical translation may remove whole-layout translation symmetry only after a checked orbit
  proof for the partial constraints. Translating the complete partial assignment toward the origin
  must preserve every partial constraint and must not be confused with fixing a full-layout
  bounding box.

No port, boundary-terminal, transport, item, flow, or logistics-component variable may be present.
Rung 0 does not materialize per-cell occupancy variables; pairwise rectangle separation is its exact
non-overlap representation. Reported bounds are `max - min` over validated facility geometry and do
not imply a translation anchor.

Rung 0 identifies rotations only up to occupied-rectangle equivalence. Rotations that produce the
same width and height are the same Rung 0 state because no directional port geometry exists yet.
Every full rotation projects to exactly one such class, and every class retains its complete set of
allowed full rotations. Rung 1 refines the class back into directional rotations when ports make the
distinction observable.

### Rung 1A: Facility Port Geometry

Adds one compatible facility-port choice and its rotated physical geometry for every facility-owned
logical endpoint. Port direction, transport kind, input/output direction, placement, rotation, and
physical port geometry remain linked by the exact endpoint-support relation.

The formulation reintroduces full directional rotation as one sparse variable per facility. That
variable is channelled bidirectionally to the Rung 0 occupied-rectangle class. Each facility-owned
terminal has independent port-choice and local-connection variables. A complete exact support
relation links `(rotation, port choice, local connection)`, and guarded coordinate equalities place
the outside-adjacent connection cell at the facility origin plus the selected local offset.

Every selected connection cell must be in the request grid. Rung 1A deliberately does not require
that cell to remain outside other facility footprints. It is an exact relaxation of the complete
port contract used only to isolate the cost of rotation-port-coordinate support from the later
physical-clearance coupling. It does not fix placement, rotation, port, or connection decisions.

Two facility terminals may select the same port or connection cell because current confirmed
semantics do not establish port exclusivity. Endpoint-to-endpoint transport collision begins with
routing and is therefore absent from this rung.

External boundary-terminal coordinates are not added. An external logical endpoint contributes only
its facility-owned counterpart at this rung. No route, flow, item-on-grid, or topology variable may
be present.

There is no invented port-distance objective. Port assignment has no independent game-quality
objective before routing exists. Like Rung 0, the primary Rung 1A experiment is feasibility-only.

### Rung 1B: Facility Port Clearance

Adds the one-cell selected-port clearance rule to Rung 1A. Every selected connection cell must be
outside every facility footprint. Validated port geometry proves that the connection is outside its
owner; exact disjunctions enforce clearance from every other facility. No other semantic state is
added.

Rung 1B is the complete facility-port contract previously called Rung 1. Comparing fresh Rung 1A
and Rung 1B models separates the cost of endpoint support and coordinate channeling from the dense
selected-connection-versus-facility clearance relation. A Rung 1A witness is not a valid Rung 1B
witness when any selected connection cell is covered by another facility.

Rung 1B has two same-semantics formulation variants. `facility-ports` is the baseline: each
endpoint, non-owner facility, and facility geometry class creates four fully reified directional
separation inequalities plus one guarded disjunction. `facility-ports-propagated` replaces that
intermediate Boolean expansion with one exact point-versus-variable-rectangle propagator per
endpoint and non-owner facility. The propagator rejects a facility geometry class when all four
separations are impossible and directly tightens coordinate bounds when a selected class has one
remaining separation. It must reject exactly the same complete assignments as the baseline and
uses the unchanged Rung 1B witness validator. The variant is a formulation experiment, not an
additional semantic rung.

The propagated rung accepts an independent `endpoint-clearance-priority` search-profile setting.
Its `High` and `Medium` values change only the Pumpkin propagator scheduling priority; variables,
constraints, reasons, accepted assignments, and validation remain identical. Priority is recorded
separately from the rung and formulation so scheduling effects can be reproduced without labeling
the same semantic problem as a different ladder rung.

The same search profile records whether endpoint-clearance diagnostic counters are enabled. This
flag changes instrumentation only; it must not change the propagated relation, event registration,
search policy, or accepted assignments. Counters remain enabled by default.

The `endpoint-clearance-false-event-filter` search-profile flag changes only wakeup scheduling.
Coordinate-bound events always enqueue the propagator. An orientation-selector event may be
skipped only when that selector is already proven false; removing an unselected rectangle cannot
create a rejection, a unique separation, or a coordinate-bound deduction. Events for true or
unresolved orientations still enqueue. The flag is disabled by default because its measured
scheduling effect is phase- and priority-dependent even though the accepted assignments are
identical.

The propagated rung also supports an exact directional-rotation partition diagnostic. The caller
selects one or more facilities introduced in the target growth phase. The harness enumerates the
complete Cartesian product of their validated directional-rotation domains and adds one
`research-fixation` equality per selected facility in each child. The children are pairwise
disjoint and their union is the unpartitioned rung. Facility coordinates, port choices, local
connection keys, endpoint coordinates, and clearance decisions remain solver variables. A
feasible child proves the parent feasible; only proven infeasibility of every child proves the
parent infeasible. Unknown children never become infeasibility evidence.

The next causal diagnostic compares the unpartitioned parent with every child at the root
propagation fixpoint through Pumpkin's no-decision root path, before any search decision. It
records the surviving values of facility
coordinates and directional rotations, endpoint port and local-connection choices, and endpoint
connection coordinates. The comparison changes no solver decision and runs the same propagated
facility-port formulation with either zero or one research-fixation equality per selected
facility.

The same diagnostic performs a conservative local-key clearance opportunity census. For every
still-supported endpoint local key and every still-possible non-owner facility orientation, it
checks the Cartesian product of the current root coordinate domains. A pair is reported as a
missed opportunity only when no coordinate tuple can place `owner + local_offset` outside the
target rectangle. Because this Cartesian product is a superset of the correlated solver states,
absence of support is a sound reason to reject the pair; presence of support is not a feasibility
claim. The census observes the current model only. It must not prune domains until a separately
reviewed exact propagator is implemented.

### Rung 2: Pipe Routing

Adds the complete pipe semantic block:

- pipe external boundary terminals;
- the shared pipe layer;
- pipe route-cell, directed-arc, arm-item, and integer-flow state;
- pipe terminal presence, item assignment, conservation, capacity, collision, connectivity, turns,
  splitters, convergers, and same-layer pipe bridges;
- facility-versus-pipe physical occupancy coupling.

All placement and facility-port decisions remain solver decisions. Belt facility-port choices from
Rung 1B remain present, but no belt grid or belt boundary-terminal variable is added.

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

1. verify that every new decision and proof obligation is required by confirmed game semantics or
   the declared output contract;
2. identify states that are already determined upstream, observationally equivalent, or irrelevant
   to witness validity and objective quality;
3. compare variable-domain volume, the exact model-family delta, root-domain pruning, the first
   branch family, conflicts, and propagation;
4. classify the evidence as a semantic-contract problem, an oversized representation, weak
   bidirectional channeling, symmetry, generic branching, or repeated semantic reasoning;
5. change the problem contract only when the current problem is proven unnecessary or incorrect;
6. compare exact same-semantics formulations only when the evidence implicates representation or
   propagation;
7. add a custom propagator only when a specific sound semantic inference is missing or repeatedly
   recomputed;
8. independently review proof soundness, implementation risk, and alternative formulations before
   committing a propagator.

After an improvement, rerun the ladder from Rung 0 through the improved cliff and then continue to
the next rung.

## Independent Implementation Boundary

Do not add another rung variant to the current `solve_with_endpoint_encoding` mode matrix and do not
construct the ladder by disabling pieces of that function. The existing shared-layer orchestration
combines endpoint encoding, research restrictions, collectors, connectivity propagation, search,
extraction, and validation. Reusing it would make it difficult to certify that a low rung contains
no future semantic state.

Build the ladder in a new independent formulation package with explicit rung artifacts:

- `placement`: exact placement formulation candidates;
- `endpoints`: facility endpoint and port-selection formulation candidates;
- `routing`: one requested transport layer and its exact topology/flow formulation candidates;
- `geometry`: active-geometry occupancy and bounds;
- `search`: cold feasibility and separately labelled optimization runners;
- `witness`: rung-specific extraction and validation;
- `experiment`: counterbalanced execution, certificates, deltas, and cliff classification.

Only semantics-neutral infrastructure may be shared with the existing implementation: prepared
`ModelInput`, validated catalogs, solver instrumentation, search counters, report serialization, and
HTML shell utilities. The old formulation remains an external semantic baseline, not a source of
hidden constraints or the implementation template for the new ladder.

Before adjacent rung deltas become authoritative, extend model recording to retain normalized
constraint descriptors. Aggregate counts alone cannot detect offsetting unintended changes.

The final rung must be semantically equivalent to the accepted full model. Prove this with the same
full witness validator, exhaustive controlled instances where practical, and bidirectional witness
acceptance tests. Structural identity with the old formulation is neither required nor expected.

## Evidence-Triggered Reformulation

Formulation choice remains an experimental axis, but it is not exercised preemptively at every
rung. Doing so would replace one large search with a combinatorial formulation tournament and slow
the ladder's primary purpose: locating the first semantic cliff.

Each new rung begins with one simple, faithful, solver-native exact encoding. After a cliff, first
check whether the solver was asked to solve the right semantic problem. Only when the retained
decisions are necessary and the metrics point to weak channeling, excessive representation, or a
solver-native constraint mismatch should the experiment compare same-semantics formulations.

Every alternative formulation must accept and reject the same controlled assignments. “Better”
means better measured behavior for the declared solver, workload family, and budget; it is not a
universal claim. A custom propagator comes later and is justified only when exact formulation
alternatives expose a specific sound inference that native propagation repeatedly misses or
recomputes.

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
