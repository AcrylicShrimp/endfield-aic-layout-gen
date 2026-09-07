# Finite action-capability probe, schema 1

Status: development experiment; no production cutover and no training yet.

## Inputs and domain

`capability.mjs --plan P --output-directory D` accepts an explicit schema-1 plan with
historical root-dataset path, named development roots, optional hash-checked replacement
witnesses, action levels and positive proposal/route-expansion/wall-time budgets. Reject
roots labeled `test`; these development observations cannot certify held-out generalization.
Output directory must not exist. Load catalogs and validator from the root dataset.

Every action starts from the same complete root. Levels are:

- `translate`: all cardinal shifts of distance 1 through `step_limit`, with singletons and
  internal-edge pairs when `max_members >= 2`. Larger levels additionally include cumulative
  upstream/downstream cones of depth up to four, bounded by `max_members` (at most 16).
  Deduplicate membership lists. This is not enumeration of all connected subsets.
- `rotate`: every non-current catalog rotation on one facility, at the current top-left
  position and optionally all cardinal shifts of distance 1 through `step_limit`.
- `port`: one facility terminal reassigned to each other compatible catalog port.
  Include occupied destinations in the raw domain, so collision rejection is visible.
- `reroute`: each extracted chain alone, plus all chains together, with near/far repair.

Retain the current extent, existing junctions and bridges; expose the chosen junction
ownership policy explicitly. No new crossings or external-input substitutions. Pipe-first
repair applies to all levels. Longer shifts, rotation+shift and larger groups are explicit
interactions, not claims about a pure one-factor experiment. Deterministic round-robin
parameter order visits each member group before deepening its shift variants.
Within each transport layer, explicit repair priority sorts original-chain endpoint distance
(ascending for near, descending for far), then chain ID. It does not recompute that priority
from a moved candidate; this fixed choice is part of the measured repair neighborhood.

## Evaluation and invariants

Enumerate the complete declared domain before trials (guard 100,000 raw actions). Charge
generation wall time. Count every selected raw action as a proposal, including screening
failures; this first probe has no policy preselection or free oracle filtering. Stop selecting
new actions when any aggregate budget is exhausted. Each routing invocation receives the
remaining aggregate expansion allowance capped by its per-action bound. Late results are
reported but not retained. Validator health timeout is independent of the experiment deadline.
Root preparation and initial validity checks precede the timed level and are not inference
work; candidate generation, screening, repair and candidate validation are all charged.

An explicit body/bounds screen reports its time. `kernel_ms` includes further static
geometry checks and routing: do not label all kernel time as A* time or hide that work.
Report actual route calls/expansions, validation calls/time and total elapsed time.

The experimental semantic identity preserves facility inventory/recipes, utilizations,
network and requirement identities, terminal commodity/rate/direction and original external
endpoint identities. Position, rotation, physical port assignment and derived components are
not frozen by this identity; independent Rust and original-request coverage validation are
mandatory. The old RL environment remains unchanged and still has its old action contract.

Only independently valid strictly better scores replace the retained root, using area,
physical transport tiles and directed turns. A valid worse candidate remains diagnostic
evidence for future two-step probes. No failure loses the original validated layout.
Verify the immutable root digest after each level. Unknown exceptions abort the experiment,
not count as ordinary action failure. These are heuristic trials, never global proofs.

## Outputs and failure taxonomy

Immutable per-root/per-level trace, retained witness and summary plus top-level provenance.
Report raw/visited domain counts, fraction visited, exact candidate identities, result counts,
unique validated states, strict improvements, best score/density and all charged work.

- `static-rejected`: explicit body/bounds or known kernel static rule; retain exact diagnostic.
- `repair-failed`: fixed routing choices failed; preserve route status and distinguish
  `work-budget` (unknown), `no-path` (only that repair context), and other fixed-choice limits.
- `validation-rejected`: generated geometry fails independent validation or root coverage.
- `unchanged`, `duplicate-valid`, `valid-no-gain`, `improved`: disjoint candidate outcomes.
- `late-result`: work completed after the level deadline; cannot replace the incumbent.

Complete enumeration means only that every action in this declared finite domain was tried.
Route unknowns, fixed routing choices and absent multi-step actions prevent a general
no-opportunity or infeasibility claim even when the entire domain was visited. These
measurements guide action/observation/learning experiments, not release certification.

The initial probe intentionally does not apply whole-grid strip compaction after every
candidate. Standalone rotation or translation cannot by itself remove a newly exposed
interior strip. Move-then-compaction is a separately declared follow-up interaction; do not
mistake this one-step diagnostic for the complete old policy or its multi-step trajectories.
