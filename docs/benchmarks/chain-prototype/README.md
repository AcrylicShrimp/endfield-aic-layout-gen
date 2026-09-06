# Chain-reasoning prototype

Status: milestones 0–5 complete. Routed chain/cycle/fan-in construction, explicit external
factories, shape-sensitive fit estimates and bounded connected replay with complete-incumbent
evaluation are implemented within the documented graph class and controls. The
[matched-budget findings](comparison-report.md) and [completion audit](completion-audit.md)
close this research goal, not the general optimizer or a production migration.
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
Those milestone-2 outputs still have explicit pending external boundaries.

See the [factory contract](factory-contract.md) and [milestone-3 report](factory-report.md) for
`run-factory.mjs`, `run-factory-controls.mjs` and the separate `verify-factory.mjs` command.
Eleven of sixteen same-cap first-priority controls produce complete factories, including all
three recipe families and both iron inventories. The default straight-first battery still
stops; only its explicit right-first control completes. These are first-completion results,
not completion-aware optimization or a universal-success claim.

See the [replay contract](replay-contract.md) and [milestone-4 report](replay-report.md) for
`run-replay.mjs` and `run-replay-controls.mjs`. Actual continuation reverses a generated iron
partial-area ordering and improves complete incumbents while preserving frozen internal chains.
Thirteen of sixteen proposal-order/ranking controls complete, but the greedy remaining-body
fit ranking fails battery and is not generally better than partial-area ranking. The explicit
failure controls remain part of the result, and no production default or fallback was changed.

See the [comparison contract](comparison-contract.md) for `run-comparison.mjs`. Its 192-run
matrix is reproducible from `comparison-plan.json`. The explicit
`replay-partial-policy.json` completes all four requests under one measured policy at the
original caps; it does not replace the original Fit policy or silently dispatch by recipe.
Actual continuation helps, but costs early completion under smaller budgets, and greedy
body-fit ranking is not generally better. All 76 complete matrix rows pass standalone audits.

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

See the [completed tracker](../../todos/2026-09-06.11-chain-reasoning-prototype.md) and the
[research decision](../user-layout-reconstruction/completion-research.md). No later milestone
is considered implemented merely because this contract describes it.
