# Chain-reasoning prototype

Status: milestones 0, 1 and 2 complete. Simple chains and atomic cycle/branch groups are
implemented; multi-input composition, complete external factories and completion-aware
improvement remain in progress.
This directory is the separate experimental entry point; production commands remain unchanged.

## Implemented entry point

See the [chain contract](chain-contract.md) and [milestone-1 report](chain-report.md) for the
geometry-free request DTO, CLI command, generated straight/bent chains, decision traces,
shared work budgets and verification. The three `*-chain.json` requests are explicit small
chain fragments, not substitutes for the full recipe-family factories required by the goal.
The [reference report](reference-report.md) retains the independently validated human layouts.

See the [group contract](group-contract.md) and [milestone-2 report](group-report.md) for
`run-group.mjs`, the geometry-free citron cycle/branch request, private incremental construction,
body-contact and 180-degree-fold proposals, and the controlled first-completion replays.
All current successful prototype outputs still have explicit pending external boundaries.

## Contract direction

The eventual constructor input is a catalog-backed production graph with stable facility,
recipe, commodity and requirement identities, exact rates/utilizations, explicit external
requirements, and bounded work budgets. It must not accept screenshot poses, selected ports,
route corners, fixed final width/height, or a complete facility-only placement as generation
input. Source screenshots are reference evidence only. Initial milestones may explicitly
select a small connected graph without claiming automatic general graph decomposition.

Output separates proposed chain relationships, attempted transforms and port selections,
committed growth states, final validation, measured score, remaining obligations, and structured
diagnostics. Each diagnostic has a stable code, severity, stage, message and relevant entity.
Decisions distinguish not-proposed, filtered, port-incompatible, route-failed, closure-invalid,
budget-exhausted, validated-but-worse, and selected. An unfinished trial has no validated
complete score. A run must never count an unvalidated candidate as its retained factory.

The basic state transition is: select a connected frontier, propose a relative pose, assign
the newly needed ports, synthesize/extend compatible commodity networks, route, validate,
then commit. An atomic cycle or approved downstream-closed group may hold several such steps
in private until closure validates. There is no all-facilities-first placement phase.

Future completion estimates may inspect remaining shapes, but do not commit ghost placements.
Chain rollback/replay owns only an explicit recent connected window and its affected routes.
It preserves unrelated validated geometry and charges all trials to the shared work budget.
Final improvement keeps the complete incumbent unless a complete validated candidate improves
area, then physical transport cells, then turns. A partial-state ranking is a heuristic, not
a proof or a replacement for that final score.

## Milestone 0 boundary

References use the separate screenshot-transcription schema. Its schema-2 input requires an
explicit utilization for every placement, an explicit item and rate for every route, and the
exact expected missing facility/item/direction margins. Existing schema-1 diagnostic inputs
must migrate; no implicit uniform-utilization or full-output-per-route compatibility path is
retained. Geometry witness schema 1 is unchanged. New files do not claim observed game operation.

The independent Rust witness probe validates geometry, port directions, capacities and recipe
margins. The reference harness additionally requires the pending margins to match the supplied
external obligations exactly. Passing means internal-reference validation, not full external
factory validation. Full-factory prototype acceptance later also requires actual external
terminals/routes, graph inventory and rate coverage, and no pending recipe margins.

This milestone reuses neutral catalog projection and independent validation, not the production
constructor or the paused breadth orchestration. Each implementation stage will define its
concrete DTOs, supported graph class, diagnostics, failure modes and test gates before coding.

See the [active tracker](../../todos/2026-09-06.11-chain-reasoning-prototype.md) and the
[research decision](../user-layout-reconstruction/completion-research.md). No later milestone
is considered implemented merely because this contract describes it.
