# Phase 3 External Boundary-Key Legal-Support A/B

## Purpose

The selected Phase 3 residual facility-port tuple case 6 remains `Unknown` after every facility
placement, rotation, and port is fixed. Its ten external boundary selectors expose a known root
propagation gap: the positive unary table contains only legal boundary geometry keys, but Pumpkin
does not eagerly remove values that never occur in the table projection.

This experiment asks whether representing the same legal key set directly as each selector's
sparse integer domain reduces the five-second first-witness cliff.

## Parent Control

The experiment reruns the accepted residual facility-port tuple portfolio and selects its reported
lowest-index combined `Unknown` case. For the accepted artifact this is case 6. Selection must fail
closed when the parent is blocked, has a witness or complete proof, lacks an unknown child, or does
not contain the selected case.

The child inherits:

- exact used dimensions `16 x 16`;
- all four fixed facility placements and rotations;
- all eleven parent facility-port assignments;
- all four residual facility-port assignments from case 6;
- the accepted sparse facility-endpoint support and exact routing propagator stack;
- the same prior exact solution used only for non-binding hints and the approved prior-placement
  fixation.

## A/B Formulations

Both formulations build one boundary selector per external terminal and retain the same unary
positive table and every downstream literal, routing option, implication, and witness check.

### A: Bounded Key Domain

Create the selector key with the current bounded domain:

```text
0 .. (4 * grid_cell_count - 1)
```

Then post the unary table containing every legal boundary key.

### B: Sparse Legal-Support Domain

Create the selector key directly with the exact `reachable_boundary_keys(width, height)` set, then
post the same unary table.

The two legal solution sets are equal because formulation A already requires the selector key to
belong to that unary-table projection. Formulation B only makes those already-required values the
declared variable domain. It does not select a boundary side, cell, route, facility placement, or
port in advance.

## Execution Contract

Run four independent solves for the selected child:

1. A authoritative feasibility-only solve, five-second budget;
2. B authoritative feasibility-only solve, five-second budget;
3. A root-observation solve, separate five-second budget;
4. B root-observation solve, separate five-second budget.

The authoritative model must not include the root observer. Search timings and counters compare
only the two authoritative solves. Observation solves provide root domains without consuming an
authoritative budget.

Every solve retains solver freedom for all ten external terminal selections and all belt/pipe
routing, material, flow, topology, capacity, collision, logistics-component, and objective
auxiliary state. Search is feasibility-only; objective auxiliary variables remain modeled but are
not optimized.

The four solves run sequentially in the recorded order in one process. This avoids simultaneous CPU
contention, but it does not counterbalance thermal or order effects. A cutoff crossing near five
seconds therefore requires a later isolated, counterbalanced repeat before it is called stable.

## Required Assertions

Interpretation is blocked unless:

- A and B reproduce the same fixed dimensions, placements, rotations, and fifteen exact facility
  terminal ports;
- one prepared logical model input and one exact fixed-port vector are recorded as a common static
  contract and cloned into all four builds; the contract includes facility, requirement, network,
  and terminal IDs and remains available even if root propagation proves a case infeasible;
- a build-time certificate exists even when search proves root infeasibility and records, for every
  external terminal, the A bounded declaration, B sparse declaration, A/B unary-table projections,
  and A/B posted routing-option key sets;
- A and B have identical external terminal ID sets;
- for each external terminal,
  `B declared values == A table == B table == A options == B options`;
- A's declared domain is independently checked against the full expected
  `0 .. 4 * model_cell_count - 1` range from the common prepared-input contract, rather than against
  bounds copied from A's own certificate;
- every B external selector's observed root domain is a subset of its declared legal key set;
- A and B have the same non-domain variable-family counts, posted constraint-family counts and
  incidences, fixed contracts, terminal IDs, and network IDs. Only the ten boundary-selector
  declared domain representations/cardinalities and the resulting propagation or presolve state
  may differ;
- no invalid witness or witness/proof conflict appears.

When both root snapshots are available, their non-domain terminal and network identities must also
match. When either case is root-infeasible, that dynamic identity check is reported as unobserved;
the common static input contract and build-time external-terminal certificates remain mandatory.
Root coverage is checked independently for each side, so a root-infeasible A does not discard an
available B observation, or vice versa.

A root-infeasible solve is valid evidence and does not require an unavailable brancher snapshot.
Static build certificates remain mandatory in that case.

Root diagnostics distinguish static legal support from propagation. They record exact A and B root
selector values, `A root minus legal`, `B root minus legal`, and legal values additionally removed
by B. `B root minus legal` must be empty. Legal values removed only by B are reported as stronger
propagation, not treated as a semantic mismatch.

## Report

Record:

- the complete parent portfolio and selected case provenance;
- formulation names and outcome for all four solves;
- build-time structural certificates and the exact A/B equality assertions;
- authoritative construction/search time, first incumbent, branch decisions, backtracks,
  conflicts, learned clauses, propagations, and model scale;
- per external terminal, declared legal key count plus A/B root cardinality and root values absent
  from legal support;
- machine-readable and HTML aggregate counts for observed A/B terminals, A/B root values absent
  from legal support, and legal values pruned only by B;
- semantic and structural assertion results;
- whether B crosses the cutoff, regresses at the cutoff, resolves alongside A, or leaves both sides
  unresolved;
- standalone JSON and HTML for the summary and all four solve layouts.

The build certificate deliberately materializes exact declared-domain values. Consequently the
reported construction times are instrumented descriptors, not a valid A/B build-performance
comparison. Authoritative search begins after construction and remains the runtime comparison.

## Interpretation

Logical evidence and performance classification are separate and symmetric:

- any invalid witness blocks interpretation;
- a validated witness from either formulation with no infeasibility proof proves case 6 and its
  selected parent feasible;
- a sound infeasibility proof from either formulation with no witness proves case 6 infeasible;
- any witness/proof conflict blocks interpretation;
- if case 6 is proven infeasible, case 12 becomes the only remaining parent leaf and must be run
  next: its proof closes the parent as infeasible, its witness proves the parent feasible, and an
  `Unknown` result makes case 12 the next cliff control;
- B resolved while A is `Unknown` is a B performance crossing and demonstrates that the legal-key
  representation contributes to this cutoff;
- A resolved while B is `Unknown` is a B regression at this cutoff, while A's sound logical
  evidence is still preserved;
- both resolve to the same evidence is a tie in resolution, with time reported descriptively;
- both `Unknown` is an unresolved runtime comparison. Root-domain reduction and changed search
  counters are reported, but fewer decisions or propagations in the same five seconds are not
  called a performance improvement because they may reflect slower propagation rather than
  greater search progress.

If both remain `Unknown`, capture B's next root state and decompose a lower routing, material, or
topology decision. If one side resolves near the five-second cutoff, counterbalance execution order
or repeat isolated paired runs before attributing a stable runtime win.

This result applies only to the selected fixed-dimension and fixed-placement child. Fixed-height
width sensitivity is a separate later experiment because changing the canvas also changes the
legal boundary relation and routing domain.
