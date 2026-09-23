# Hierarchical policy stability factorial

Status: experiment design fixed before training. This is a
bounded diagnosis of the outer-policy instability documented in
[`../reports/2026-09-23-hierarchical-stability-diagnosis.md`](../reports/2026-09-23-hierarchical-stability-diagnosis.md),
not a production learner selection or an ultra-long launch.

## Question and hypotheses

Two mechanisms can make a newly trained outer policy forget useful macro
actions: replacing each generation's training set with new roots, and treating
the completed-Q policy distribution from failed searches as a fully trusted
teacher. Crossed checkpoints and final-layer ablations identified the outer
policy as the behavioral source of case churn, while target audits found that
failed-root targets placed about 73-75% of their mass on unvisited actions.

The experiment compares two binary factors from the same generation-87 lower
and outer checkpoints:

| Arm | Successful-root replay | Failed-root policy loss |
| --- | --- | --- |
| A | Off | On (historical target rule) |
| B | On | On |
| C | Off | Off |
| D | On | Off |

Every arm retains value supervision from failed roots. This is a diagnostic
success replay treatment, not the positive-advantage rule of full SIL. A replay
root is admitted only if its collected episode was independently validated.
Removing failed-root policy terms retains the original full-batch denominator,
so each successful root's policy-loss weight stays unchanged.
Replay is bounded to the previous four committed generations and replaces half
of the current outer-policy training slots when available. Each arm keeps the
same 256 outer root-state slots per epoch, 16-state batches, 200 epochs and
optimizer steps,
training-panel cases and search simulations. The lower model is frozen, so its
checkpoint bytes stay identical in all arms. No beam fallback, relaxed physical
validation or curriculum promotion is introduced.

## Fixed inputs and separation

Use one level with exactly two facilities, up to six terminals and six
connections. Start from committed generation 87 of the stopped corrected run;
never resume or mutate that run. All arms receive the same immutable 32-case
training panels at each generation, same Gumbel search seed and limits, same
model initialization, and eager inference. Arm identity affects output paths
only, not case IDs or sampling seeds.

Prepare a fixed 64-case diagnostic panel from the **train source partition**
with a distinct namespace and seed. Its problem identities must be disjoint
from every arm's training panels. It is used for paired model comparison, never
for optimizer updates. The former 128-case validation panel is historical
diagnostic evidence and is not read during this screen. A new untouched final
panel will be prepared from a separate source partition only after a treatment
and its threshold have been fixed.

The train source has recurring graph identities across panel seeds. Prepare
256 diagnostic candidates using seed `20261000` and namespace
`hierarchical-policy-stability-diagnostic-v1`; retain the first 64 whose
canonical problem JSON hashes are absent from all eight common training panels
and not already retained. Preserve the 256-candidate source and selected panel
hashes in the experiment manifest.

The existing search-report, panel, model and optimizer formats remain the
recovery authorities. Optional training controls are bound by the curriculum
and generation configuration hashes. A persistent in-memory replay window
holds successful `OuterExample` values moved from each committed generation;
resume rebuilds it once from the same validated panel and collection files.
Recovery binds the ordered panel and collection hashes in that window. The
normal no-replay path keeps the owned in-memory handoff and its current I/O
behavior. A single generation must not reread its just-published collection.

## Measurements and decision gates

Use a short two-generation arm smoke to verify identical panels, frozen lower
checkpoint identity, equal outer update counts, actual replay after the first
generation, distinct arm controls, search publication and resume. Then run an
eight-generation, one-seed four-arm screen.
Evaluate the fixed diagnostic panel after generations 1, 3, 5 and 7. The
screen is for mechanism diagnosis, not a launch decision.

Record per-case validation, gains and losses against the common start and prior
evaluation, facility/connection strata, no-macro-actions, root policy entropy,
maximum prior, target mass on unvisited actions, replay fraction, successful
and failed policy-root counts, update count, parameter movement and integrated
wall time. Conditional completed-layout quality remains secondary to
completion. Summarize the final two evaluations' mean and minimum completion
and their losses of previously solved diagnostic cases; do not choose a
checkpoint by its single best result.

Interpret the matrix before changing another control:

- B improves over A and D over C: retention is supported.
- C improves over A and D over B: failed-root targets are implicated.
- Only D improves: the two mechanisms interact.
- No arm improves with acceptable retention: test outer update magnitude next,
  keeping this factorial closed; do not add more factors to it afterward.

One seed cannot establish stable superiority. A promising contrast must be
repeated with at least three predeclared training-panel streams from the same
start, followed by a longer integrated run with lower training restored. Use
paired per-case changes and variation across streams; preserve easy controls
and require the diagnostic minimum as well as mean to improve. The numeric
long-run gate is fixed from the screen's baseline variability before those
confirmations, not chosen after seeing their outcomes. No ultra-long run is
authorized by this screen alone.

## Continuity and failure controls

| Existing benefit | Treatment | Check |
| --- | --- | --- |
| Exact native search and validation | Keep the search and physical publication paths | One bounded complete search in every arm |
| In-memory fresh collection handoff | Move successful outer examples into a bounded cache | Integrated timing and no fresh collection reread |
| Atomic generation and optimizer recovery | Bind replay source identities and rebuild cache once on resume | Interrupted-generation resume smoke |
| Training/evaluation separation | Diagnostic source is train but identities remain disjoint | Explicit identity audit before launch |
| CPU/GPU resource limits | Four-generation replay cap, equal root-state slots, persistent models | Host/GPU and disk preflight plus wall measurement |
| W&B nonblocking tracking | Attach each unattended arm before launch | Acknowledged real readiness record |

The screen stops on a schema/identity mismatch, missing replay source, unexpected
lower-checkpoint change, invalid layout, unequal update budget or exhausted
resource guard. A bounded failure is reported as such, never as infeasibility.
