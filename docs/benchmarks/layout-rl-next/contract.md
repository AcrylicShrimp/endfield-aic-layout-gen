# Continued-improvement RL experiment contract

Status: designed, 2026-09-07. This is isolated research, not a production cutover.

## Question and scope

Can broader complete-factory training and longer trajectories teach repeated improvement,
including improvements late in a rollout and on already compact states? The eventual goal
is a tightly packed factory; this experiment cannot certify that no better legal layout exists.
It preserves the existing action, routing, validation, model and lexicographic score contracts.
Rotations, port identities and the current-extent routing restriction are unchanged.

The existing constructor supports at most 16 facilities in a single-transport upstream tree
or its supported two-node cycles. This is a diagnostic implementation restriction, not a
game limit. Do not change it while measuring the effect of data and trajectory length.

## Corpus stage

CLI: `node docs/benchmarks/layout-rl-next/corpus.mjs --plan P --output-directory D`.
All paths in the plan resolve relative to the plan; the output directory must be new.

Input: versioned plan with external catalog paths, validator, previous imported corpus,
explicit product-family split assignments, root utilization, recipe expansion depths,
constructor variants, per-construction work limits and total elapsed guard.
Recipe choices and rates are derived from current external catalog data, never embedded
in program code. Procurement depth is part of each **new request**, fixed before placement.
At that depth, intermediate inputs are explicit external obligations. These requests do not
replace or relax any earlier factory request. Oversized or unsupported requests are rejected,
not cropped to obtain a successful factory.

Output: `roots.json` in the existing environment DTO and `report.json` with every attempted,
duplicate, unsupported, stopped or unattempted row, construction budgets, semantic/family
identity, geometry fingerprint, fresh geometry and original-request coverage audits, and
input/implementation hashes. Complete construction reports and requests are retained per row.
Previous roots keep their exact split and geometry. New product families are split before
construction; all depth, pose and descendant variants inherit the family split. Detect exact
geometry equivalents across splits and reject leakage. Shared upstream recipes/topology
motifs are allowed and disclosed; family holdout is not a claim of disjoint graph motifs.

Only complete independently validated factories enter training. Constructor failures remain
first-class rows; failure rates are not optimizer quality. No imported factory receives a
new W/H constraint. Producer guards, output size and measured footprint are separate facts.

Diagnostics retain `code`, `severity`, `stage`, `message`, and `entity`. New codes use the
`rl-next-` prefix. Important outcomes: `input-invalid`, `unsupported-request`,
`construction-stopped`, `corpus-budget`, `duplicate-root`, `split-leakage`, `validation-failed`,
`stale-input`, and `corpus-gate`. Partial/failing corpora must not silently start a full run.

## Learning and evaluation

Use the existing CPU graph policy and terminal-comparison policy gradient. A step is one
charged actual modification attempt, including rejected and unchanged attempts. An episode
continues after improvement; its private state may worsen while its best valid state is retained.
Compare fixed horizons 16, 32 and 64 at equal total training action budgets, not equal episode
counts. Separate data breadth from horizon. Rehearsal of small factories is retained.
Fresh initialization is used in these controls; the previously trained model is not silently
fine-tuned, because that would add unequal historical training and prior test exposure.

No test-based checkpoint selection. A pilot uses training and validation only. Before full
evaluation, freeze seeds, methods and proposal/time budgets. Record best-score curves, first
and last improvement steps, accepted/rejected/unchanged/worse transitions, no-improvement tail,
route work, elapsed time and exact final score. Audit every final witness independently.

`proposal-budget`, `elapsed-budget` and observed stagnation are not optimality certificates.
A future residual search must start from the **same learned incumbent**, charge its own work,
and distinguish: improvement found; sampled neighborhood exhausted; resource-limited unknown;
or a precisely scoped local proof. A failed heuristic search cannot certify even local optimality.
No learned STOP action or stagnation-based training termination is introduced in this slice.

## Architecture review

Compared with `docs/designs/2026-09-02.00-constructive-layout-planner.md`: complete factories
are constructed incrementally, each exported improvement remains complete and independently
validated, score order is area/physical tiles/turns, and no production entry point or global
exact solver is changed. This experiment adds data and bounded measurements, not a fallback.
