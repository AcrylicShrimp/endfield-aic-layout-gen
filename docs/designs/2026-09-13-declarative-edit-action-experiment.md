# Declarative edit action experiment

Status: complete. Registered before running the random comparison. The
[findings and continuity closure](../reports/2026-09-13-declarative-edit-action-experiment.md)
record actual verification and measured scope. This is an explicit diagnostic,
with no learner update or production/training cutover.

## Question

The user authorizes a facility decision containing position, catalog rotation and
physical port assignment, plus row/column insertion and deletion. Routing,
complete validation and retaining a better result belong to the environment.
There is no policy repair, repair-all, commit or restore-best action in the new
contract. Compare this representation with the retained explicit-action baseline
before considering any training experiment.

An improvement remains lexicographic used area, transport tiles, route turns,
then existing tie-breakers. Routing search failure is not layout infeasibility.
A single random-action success probability depends on its sampling distribution;
it is not an intrinsic measure of the action representation or learnability.

## Continuity checklist

| Established capability / reason | Placement in this experiment | Required evidence |
| --- | --- | --- |
| Fresh production products, rates and fully randomized complete starts | Existing external-data random generator; freeze each admitted pair for every arm | Record all draws, generation failures and exact input hashes; no fixture fallback |
| Complete construction before improvement; no global exact solver | Admit complete native-validated factories into an isolated editor | Actual CLI admission and final witness validation |
| Immutable directed incidence, identities, items, rates, recipes and utilization | Original blueprint remains immutable and separately observable | Branch/merge/bridge fixture and independent directed-chain export audit |
| User now permits port changes | Draft owns physical port assignment; facility endpoint identity stays fixed | Joint pose/port change, compatibility and duplicate-port rejection |
| Original port-adjacent turns and component channel directions | Common-action comparison preserves old routing choices; new port choices must allow legal immediate turns | Retained port-turn fixture and component direction checks |
| Multi-action unfinished drafts, including overlap | Valid syntactic edits persist even if automatic routing fails | Overlap, later clearance, exhausted route work and interrupted stop |
| Valid best and area-first scoring | Auto-validate changed complete candidates; save only a strict score improvement | Known improvement and valid worse candidate; independent final admission |
| Persistence of actual inputs, draft, best, counters and time | New schema-specific native checkpoint, no old-checkpoint adaptation | Restore incomplete port/grid draft and continue; mismatch/forgery rejection |
| Hard operational work, coordinate and time guards | Bounded local A*, episode and command limits | Exhaustion diagnostics and no mutation after closure/deadline |
| Expensive audits stay isolated | One-off probe and comparison commands; identity checks at admission | Report runtime, validation counts, worker cleanup; no learner hooks |
| Existing accepted source and old policy evidence | Old session remains solely as explicit experiment comparator | Old tests and scripted common-action equivalence; no relabeling old models |

## New stage contract

`declarative-edit-v1` admits an original request, complete witness, external
catalogs and explicit action/time/routing limits. Observation includes original
blueprint/hash, current poses, selected ports, moved anchor/component geometry,
paths and pending edges, conflicts, best metrics and resource counters. The
original blueprint is never rewritten to disguise a topology change.

Current nodes contain current positions and component references. Original
terminal metadata belongs to the immutable blueprint; current selected ports
belong to `port_assignments`. Do not duplicate a partly updated terminal DTO
inside current nodes with its old `position` field.

Actions bind original blueprint hash and current revision:

- `place`: facility ID, x, y, allowed rotation, and a complete mapping from that
  facility's original terminal-node IDs to compatible physical port IDs. The
  mapping is applied jointly; swaps must not depend on assignment order.
- `insert_band`: axis x/y, coordinate and positive integer width.
- `delete_band`: axis x/y, coordinate and positive integer width.

A band transforms all later facility bodies, original components and external
anchors together. Insertion cannot slice a rigid body. Deletion cannot remove any
part of a facility, component or external anchor; transport cells in the deleted
band may disappear and affected connections are automatically repaired. No entity,
port obligation, edge, component channel, external obligation or rate is removed.
These are coordinate edits, not facility resizing or topology edits.

After a changed edit, detach invalid paths and route pending original edges pipe
first with one shared action budget. The repair batch is transactional; a failed
batch leaves the accepted placement/port/band edit and pre-repair paths intact.
Complete conflict-free candidates receive native full validation and an automatic
best update. A syntactic rejection changes neither draft nor best. Identical edits
consume a decision but do not repeat routing or validation. A returned or exported
ordinary result is always the retained validated best. The explicit diagnostic
draft-export method requires prior complete native validation. No-op detection does not authorize
unbounded caching, repeated source audits or expensive per-action proof replay.

The new checkpoint binds the original request/witness and catalogs, separately
stores draft and best, and preserves exact action count and elapsed allowance.
Old checkpoints and old policies are explicitly incompatible. No training path
uses this experiment until a separate cutover is authorized and verified.

Stable diagnostics contain code, severity, stage, message and entity. Report
syntactic rejection, unchanged edit, pending routing, overlap, route exhaustion,
physical validation rejection, valid non-improvement, saved improvement and
resource termination separately.

## Registered comparison

Generate the first sixteen admitted distinct starts from a new seeded stream,
with at most thirty-two generation attempts. Products and rates are drawn by the
existing four-product generator; do not select on editing success. Report product,
size, rate and geometry coverage and every failed generation/admission. Freeze
actual inputs once, then pair all arms on them. No training inputs are consumed
or changed and no previously improved factory is substituted into this panel.

Before any random run, set the native total routing ceiling to 3,200,000
(64 decisions times 50,000), so the common scripted and automatic arms receive
identical per-decision allowances. The lower work prefixes remain separate
comparisons. This avoids changing the old session's immutable configuration or
silently giving its last repair more work than the automatic arm.

Use four action streams per factory, each with at most sixty-four edit decisions.
Register all random seeds and action distributions in an external experiment
configuration before running. Common proposals use the same facility and target
pose stream; rotation remains unchanged and original ports are retained.

Arms:

1. `legacy_random`: old kind-balanced random pose/repair/repair-all/commit/restore.
   This measures orchestration waste, not a matched placement-only causal effect.
2. `legacy_scripted`: common placement, then old repair-all and commit. Charge all
   three native attempts and their work, while counting one placement decision.
3. `automatic_common`: exactly the common placement stream, automatic routing and
   saving, retaining old routing orientation semantics for the common domain.
4. `automatic_rotation`: same placement proposals plus sampled catalog rotations.
5. `automatic_ports`: joint placement, rotation and compatible port assignment.
6. `automatic_grid`: joint placement/rotation/ports plus inserted/deleted bands.

The old scripted and automatic common arms must agree on complete result quality
for identical decisions unless a documented contract difference explains it.
Report real native attempts, A* and preflight work, routing calls, validation count,
editing CPU/wall time and final score. Compare equal edit-decision prefixes and
results under common cumulative routing-work and measured execution-time budgets;
do not equate one new decision with one old primitive action.

Also sample independent one-decision proposals from the original starts, keeping
common placement draws paired and reporting the new action families separately.
This measures immediate opportunities without confusing an already damaged draft
with the initial factory. Sequential runs separately measure first improvement,
best retained quality at decision prefixes, and recovery after unfinished drafts.
No per-step re-admission or independent topology replay belongs inside timed runs.
Use per-factory paired totals, not thousands of correlated actions as independent
factory samples. Report descriptive results and scope; do not infer learnability.

## Gates and stopping conditions

Before the matrix: pass semantic contract probes, checkpoint/failure probes,
retained predecessor checks and actual CLI smoke. Use release native code, one
CPU thread per worker and at most eight distinct CPU cores. Keep generated inputs,
selected witnesses/checkpoints and compact raw results under ignored artifacts.
The external configuration specifies per-action route work, per-episode time and
work, generation time, command wall/RSS and total probe caps. Resource exhaustion
is a recorded incomplete/bounded result, never an implicit success or retry with
larger limits. End the isolated processes after the bounded screen.

Complete with measured findings, limitations, concise provenance, required
workspace/artifact checks and a focused commit. A production or learner cutover
and expensive training are outside this experiment.
