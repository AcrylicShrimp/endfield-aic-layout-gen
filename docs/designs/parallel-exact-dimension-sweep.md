# Parallel Exact Dimension Sweep

## Status

Accepted research implementation contract. This orchestration wraps independent instances of the
unchanged joint placement-routing formulation. It does not replace solver decisions with a layout
or routing heuristic.

## Goal

Obtain a feasible used-area upper bound quickly, use that incumbent to stop launching dominated
larger-area partitions, and prove the primary used-area optimum when every smaller exact partition
has been proven infeasible.

## Candidate Domain

The sweep uses the complete exact `(used_width, used_height)` candidate list defined by
`exact-used-dimension-partition-diagnosis.md`.

Candidates may be absent only when a game rule or sound relaxed model proves them impossible. The
initial implementation uses the existing minimum rotated facility dimensions, facility-area sum,
and mandatory non-facility transport-cell lower bounds. Candidate order is area, maximum side,
width, then height.

Ordering affects latency only. It must not remove a legal candidate.

## Parallel Scheduler

The caller supplies a positive worker count. Every worker owns an independent Pumpkin solver
instance and solves one fixed-dimension feasibility problem at a time. Workers share only:

- the area-ordered crossbeam work queue;
- an atomic best feasible area;
- a crossbeam completion-event channel.

When a worker validates a witness, it atomically lowers the best feasible area before publishing
the completion event. Other workers therefore observe the new upper bound before taking their next
case; they do not wait for periodic polling or for the coordinator to finish processing a report.
The coordinator consumes completion events immediately and records upper-bound improvements in
arrival order.

Before starting a queued case, a worker reads the current best feasible area:

- candidate area less than or equal to the incumbent area: execute it;
- candidate area greater than the incumbent area: record it as skipped above the proven feasible
  upper bound.

A case already running when a smaller upper bound arrives is allowed to finish. Its result remains
diagnostic evidence but cannot replace a smaller-area incumbent.

The first implementation does not interrupt Pumpkin mid-search and does not share learned clauses,
partial assignments, placements, ports, or routes between workers.

## Upper-Bound Semantics

Only a complete independently validated witness may update the shared upper bound. If a witness has
area `A`, the primary optimum is at most `A`; unstarted cases with area greater than `A` are
dominated for the primary objective and may be skipped without quality loss.

A timeout, `unknown`, invalid witness, partial layout, or unvalidated incumbent never updates the
upper bound.

## Primary Optimality Proof

The sweep reports the primary area optimum as proven only when:

1. at least one validated witness exists at final upper-bound area `A`; and
2. every candidate with area less than `A` terminated with a proven-infeasible result.

No smaller candidate may be skipped. An `unknown` smaller candidate leaves the result as an
unproven upper bound even when a larger witness exists.

The sweep does not prove secondary objectives. Same-area feasibility witnesses may be compared as
observed incumbents, but transport-tile, turn, shape, and component optimality require subsequent
optimization across every relevant same-area dimension partition.

## Reports

The stable research report records:

- request ceilings and selected network identity;
- lower-bound components and the full exact candidate list;
- requested and actual worker count;
- equal per-case search budget and outer wall time;
- each candidate's disposition: executed or skipped above upper bound;
- each executed case's exact solver report;
- every upper-bound improvement in observed completion order;
- final feasible upper bound when available;
- unresolved smaller candidates;
- whether primary area optimality is proven;
- the selected observed incumbent and its full validated layout.

The CLI writes summary JSON plus per-case JSON and self-contained HTML. Skipped cases have no layout
artifact. If no validated witness exists, the summary HTML renders structured failure evidence.

## Exactness And Non-Goals

The sweep is an exact decomposition because every legal complete layout belongs to one exact
dimension partition. Incumbent pruning removes only cases that cannot improve the primary area
objective.

This slice does not add:

- a guessed lower-bound cutoff;
- a preferred aspect-ratio cutoff;
- deterministic or randomized placement;
- port preselection;
- routing corridors or path construction;
- a separate router;
- cancellation of running solvers;
- clause sharing;
- production cutover;
- secondary-objective optimality claims.

## Failure Semantics

- no witness and at least one `unknown`: no upper bound, primary optimum unproven;
- no witness and every case proven infeasible: complete problem proven infeasible;
- witness plus unresolved smaller cases: feasible upper bound, primary optimum unproven;
- witness plus every smaller case proven infeasible: primary area optimum proven;
- worker panic or invalid witness: structured research failure, never silently converted to a
  solver result.

## Verification

- deterministic candidate ordering and completeness tests;
- scheduler test proving no case below the final upper bound is skipped;
- scheduler test proving larger unstarted cases are pruned after a validated witness;
- optimality-status tests for proven-infeasible and unknown smaller cases;
- model identity checks showing every executed case differs from the free model only by two
  dimension equalities;
- `cargo fmt --all`;
- `cargo check --workspace`;
- `cargo test --workspace`;
- optimized release execution with at least two OS workers;
- comparison against the existing sequential fixed cases and unfixed feasibility baseline.
