# Production-ready learned improvement acceptance contract

Status: unmet release goal; incremental work resumed with explicit user authorization,
2026-09-08. No model is certified production-ready. Acceptance requirements remain
unchanged. Follow the [active staged goal](../../todos/2026-09-08.05-factory-improver-goal.md);
the [2026-09-07 handoff](../../handoffs/2026-09-07-layout-learning/README.md) remains
historical evidence and does not authorize resuming its old training streams.

## User target and invariants

Heavy Xiranite is `item-xiranite-enr-powder`, as identified by the external Korean localization
catalog. The mandatory reference request is the existing 59-facility graph producing 0.1/s,
with its original water/sewage inputs and low-polymer surplus export. Use the original request
in `constructive-chain-blocks/gap-1-placement.json` and the full-graph coverage validator;
do not substitute a smaller production graph or additional external intermediates.

The user requires a model that works well on diverse arbitrary factories, wastes few actions,
and reduces internal unused area in the mandatory Heavy Xiranite factory to at most 1% of
total area. Actual rendered geometry must look densely packed with no gratuitous wiring.
Numerical success alone is insufficient. The prior 41x52 heuristic incumbent and the learned
incumbents must all be measured; do not compare only against the loose original constructor.

All exported layouts must retain the complete original factory semantics and pass independent
geometry, recipe, capacity, reachability and original-boundary validation. A failed proposal
cannot lose the best validated layout. Preserve score priority: used area, physical transport
tiles, directed turns. Production commands must not depend on the global exact solver.
No density-driven filler routes, duplicate facilities, fake external endpoints, cropped bounds,
hidden pending requirements or excess production may be used to pass the gate.

## Metrics

The used bounding box includes all facility bodies, belt/pipe cells and logistics-component
geometry. The empty-cell fraction is `(box area - occupied XY union cells) / box area`.
Independent belt/pipe overlap counts once for XY occupancy; physical transport tile cost
still counts per layer. Count all empty cells inside the box, not only enclosed holes.
Report facility-only occupancy and transport occupancy separately, so extra wiring cannot
conceal poor facility packing. Also report the largest empty axis-aligned rectangle.

Mechanical futile actions are charged proposals that are rejected, geometrically unchanged,
or return to an already visited identical state. A valid temporary deterioration is not
automatically futile: it can enable later improvement. Report these categories separately.
Report preflight candidate rejection and its CPU work separately; masking cannot make
expensive hidden search disappear. Zero selected actions are not evidence of efficiency.
Routing length above an obstacle-free endpoint lower bound is a diagnostic, not proof that
the detour is redundant. Necessary obstructions and cyclic production must be respected.

## Operational release gates

The mandatory 1% and visual gates are user requirements. The numerical interpretation of
"few futile actions" and the diverse-case test envelope below are working release thresholds,
fixed before new test evaluation rather than selected after observing results:

- 100% independently valid retained outputs and no semantic/input mutation or lost incumbent.
- Mean per-factory mechanical futile-action rate at most 10%, and 95th percentile at most 20%.
- A sealed suite of at least 300 procedurally sampled complete-factory requests spanning
  small (2–16), medium (17–64), and large (65–128) facility groups; belt, pipe and mixed
  transport; branching/merging and supported cycles. These are test strata, not game limits.
  Related recipe families and all layout descendants stay in one split. Record construction
  attrition; unsupported required strata fail readiness instead of disappearing from the suite.
- Compare retained-initial, uniform, untrained-model, geometric-search and learned controls
  at the same charged work and time. Require a demonstrated learned benefit across held-out
  families, not best-of-seed selection or one cherry-picked factory. Freeze the precise
  quality-margin statistical test after dataset feasibility, before opening the sealed suite.
- Heavy Xiranite at most 1% XY empty area, plus direct visual approval of its full plan and
  detailed junction/return-path regions. Independently inspect fixed random held-out samples
  and worst density/route/waste outliers. Every inspected image must match its witness hash.
- CPU CLI inference, model version/hash/schema checks, explicit failure diagnostics, deadline
  and memory guards, deterministic replay where applicable, and production integration tests.
  No training process or Python environment is silently installed by a production CLI.

These gates certify only the declared layout-validation scope. Power coverage, bootstrap/startup,
and unsupported game features must remain explicit rather than being claimed from steady-state
geometry. Resolve any required product-scope expansion before calling the model production-ready.

## Verdict and immediate stop policy

`not-ready` means a measured release gate fails. `unknown` means evidence/budget is insufficient.
`implementation-blocked` identifies a concrete missing capability. None alone proves impossible.
An `impossible` verdict must name the exact conflicting requirements, fixed semantic assumptions,
and reproducible counterexample or sound proof. For a local proof, identify the precise local
domain; it is not proof about all legal layouts. Stop further training/implementation immediately
when such a goal-level impossibility is established, preserve evidence and report it to the user.
Do not keep training merely to avoid a negative verdict; do not invent one from a timeout.

## First diagnostic stage

CLI: `node docs/benchmarks/layout-production/baseline.mjs --plan P --output-directory D`.
Input: schema-1 explicit witness references with hashes, original request, loaded catalogs,
validator and optional episode reports. Output: immutable `report.json` with independent audits,
density/empty-rectangle/route diagnostics and precise scope, plus provenance. Output paths must
be new. Errors use `ready-` codes and include severity, stage, message and relevant entity/path.
Pure metric routines never mutate source witnesses and never assert feasibility from density.
