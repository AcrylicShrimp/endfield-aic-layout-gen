# From-scratch quality-weighted replay comparison

Status: user-authorized collision replacement and own-arm continuation accepted after independent review (PASS_WITH_NOTES). The first implementation stopped on verified leakage. The user explicitly replaced the
trained-checkpoint transition experiment with training from scratch.

## Estimand and excluded evidence

Compare replay methods applied throughout learning from random initialization.
No scientific training arm may import a trained model, optimizer state, replay
entry/priority, native draft, historical record, input curriculum plan or learned
initial layout from the main run or a previous experiment. Reusing a numerical
architecture/configuration and external game data is allowed; inheriting learned
state is not. The main update-1908 run remains paused and is read only for a
before/after integrity check. Its checkpoint is never a training initializer.

The former coverage-gated and warm-start studies' outcomes, shortlists and panels
are excluded from scientific selection. Retain their stopped checkpoints and
withdrawal records; reuse tested infrastructure and engineering cost evidence.
The warm-start run stopped after 348 updates and has no final winner. Its source
is archived in commit `2c49dc7` and in its unchanged server source directory.

This is a bounded early-learning comparison, not proof of convergence or a
universal optimum. Report learning curves, actual mechanism activity and variation
between initializations. An inactive retention mechanism cannot be called useless;
report that its effect was not exercised. Such observations never exclude an arm
or trigger an outcome-dependent extension.

## Initialization and controls

Register initialization seeds **71 and 113**. All eight development profiles start
from the same seed-71 random parameter values and empty Adam states. Each fresh
seed-113 arm starts from its own random initialization using that seed; the three
seed-113 arms must match each other and differ from seed 71. Both seeds use the
normal behavior/shuffle seed formulas from `Learner(saved=None)`, rather than the
warm study's sampler-only reseed. They share paired generated input slots, so call
them two distinct initializations with paired inputs, not independent input sets.

Every training command starts at update 0 with `Buffer()` empty, zero replay
counters, no source rows or historical training/evaluation ledger, no native
history and no curriculum donor. The 80/10/10 curriculum stays enabled: its normal
initial fresh substitutions apply until the arm itself produces eligible parents.
Later inherited starts use only that arm's immediately preceding wave. Never copy
another arm's policy-transformed starts. No common learned warmup is allowed.

Before first collection, record source/configuration identity, seed, model hash,
empty actor/critic state, empty complete buffer and empty learning history. Reuse
the normal initial checkpoint; do not add a parallel snapshot journal. Normal
initial-plan assignment may populate the zero-action stream state at update 0,
but cannot change the initial model/optimizer/buffer proof. Validate all eleven
training initializations against these invariants and seed-pair bindings.

Keep the existing 32 logical streams, 32-action chunks, 64-action episodes,
production physical worker allocation, packed CUDA PPO/SIL, native limits,
external-data generator, model, optimizer, KL stopping, returns and SIL coefficient.
Copy numerical settings into the registered cold-study configuration, then remove
historical duration/endpoints/locators. The scientific CLI has no training-parent
argument and rejects warm study state. A new artifact schema makes this cutover
explicit. Recovery may restore only a checkpoint created by that same cold arm;
this is not a new scientific replicate or a cross-profile initializer.

## Profiles and allocation

All eight hypotheses are retained without using withdrawn rankings:

| ID | Retention | Sampling | Sequences/minibatch |
| --- | --- | --- | ---: |
| baseline-8 | v1 | Balanced uniform | 8 |
| uniform-16 | v1 | Balanced uniform | 16 |
| uniform-32 | v1 | Balanced uniform | 32 |
| gain-8 | v1 | Existing 50% uniform / 50% native-gain score | 8 |
| score-8 | v1 | Existing rare-3pp mixture | 8 |
| geometry-8 | v1 | Existing capped-exterior mixture | 8 |
| priority-8 | v1 | Existing positive-advantage reference | 8 |
| protected-score-8 | v2 q=.10, grace=64, area order | rare-3pp mixture | 8 |

Capacity remains 4,096 entries / 262,144 labels; SIL remains .05. Apply each profile
from the first update. No historical priority bootstrap or metadata backfill is
permitted: new terminal admissions create all required metadata and priorities.

1. Train all eight seed-71 profiles from 0 through **128 updates**. Evaluate the
   development panel at 0/32/64/96/128. Select by update 128, never an intermediate
   peak, replay coverage, loss magnitude or arbitrary minimum improvement.
2. Write an immutable receipt selecting the two highest mean nonbaseline
   development endpoints, even below baseline. Baseline always confirms. Ties
   within 1e-12 percentage points prefer fewer samples, fewer changes, then ID.
3. Evaluate the already-trained seed-71 baseline and finalists at update 128 on
   the new confirmation panel. This is read-only endpoint evaluation, with no
   training or optimizer updates and no copy into another training arm.
4. Train those three profiles from random seed-113 initialization through 128
   updates, evaluating confirmation at 0/32/64/96/128. No model, Adam, replay or
   curriculum state crosses from seed 71. All three seed-113 arms start at zero.

Total scientific training: **8×128 + 3×128 = 1,408 updates**. Final comparisons use
both 128-update model endpoints. This avoids selecting a retention variant solely
during an initial short buffer-fill interval; report the observed fill, evictions
and protection ages rather than asserting they must occur on a schedule. It does
not require success saturation or a minimum cohort count.

## Inputs and evaluation separation

Use a newly initialized append-only fresh bank with training generation seed base
**718900000**, keeping the existing canonical slot mapping and generation contract.
Profiles and initialization seeds pair actual fresh files by global slot. Import
and synchronize only immutable generated files, reject conflicting content and
preserve each arm's own inherited starts. Do not reuse any historical training
bank as the new bank's source.

Generate two new 32-factory panels, 16 per size stratum, with two action replicates
per geometry. Development generation/action seed bases are **718200000/718300000**;
confirmation uses **719200000/719300000**. Each panel allows 128 generation attempts
and admits first distinct native-valid geometries, without an edit-outcome filter.
Exclude all prior panel geometries, including both withdrawn warm-study panels,
and reject overlap with actual new training or the other panel. Prior panel files
may be read only for identities. Never silently remove overlaps from a mean.
Keep evaluation at 64 actions / ten seconds and the unchanged native routing limits.

Confirmation evaluation is inaccessible until the selection receipt is persisted.
Bind each evaluation to its panel, checkpoint/model, profile and initialization
seed. A pure evaluation command must prove it made zero training updates and
left the endpoint checkpoint unchanged. Reuse one initial evaluation per matching
initial model and panel across profiles; initial evaluations from different model
seeds or panels are never interchangeable.

## Quality and analysis

Outcome quality of each successful edit remains its actual validated area gain
relative to the preceding retained-best area. Keep heuristic score, native quality
and loss/contribution distinct. Record terminal attempts before eviction, draws,
quality-weighted draw mass, eligible-pool quality, stage-balanced raw/corrected SIL
contribution, positive/zero label mass, sequence lengths and per-category counts.
Preserve existing measurement transfers; add no model forward pass for observation.

There is no initial success population from which to estimate quartiles. Use fixed
**native-gain bins with cuts .01/.03/.10** for descriptive allocation, explicitly
labelled as bins rather than empirical quartiles. They do not alter sampling,
retention, selection or adoption. Report continuous means and newly observed
terminal gain distributions by stratum and facility/deletion category, including
empty bins. No pretraining is introduced to manufacture a reference pool.

Average action replicates within geometry, geometries within stratum, the two
strata equally, and the two initialization endpoints. Rank actual confirmation
mean area reduction. Report each initialization and stratum, paired differences,
win/tie/loss counts, native validity/guards, learning curves and real costs.
Use 10,000 whole-geometry stratified bootstrap resamples, seed **718600000**, with
both action/model endpoints attached to each geometry. These intervals describe
panel variability conditional on two observed initializations; they do not
estimate broad initialization variance. Development used seed 71, so separately
report the fresh seed-113 outcome to expose selection sensitivity.

A best observed setting is not conclusively superior when uncertainty or seed
variation is large. Preserve the observed ranking without a five-point hurdle.
Missing data, source/initialization violation, leakage, worker failure or an
unfinished hard budget yields incomplete evidence and no final winner. No
automatic adoption, main restart or extension after observing results.

## Continuity, readiness and resources

| Existing benefit | Reuse/cutover | Verification |
| --- | --- | --- |
| Correct joint policy, true terminal credit, packed CUDA and persistent workers | Unchanged normal learner and execution kernel | Existing native/CUDA evidence plus affected observer/initialization tests |
| Empty-start and 80/10/10 bootstrap | `Learner(saved=None)`, `Buffer()`, normal initial/next plan | All initializers empty; own-arm donor identities; no foreign history |
| Batched replay/source caches | Existing normal update/publication | Actual run timing; no historical input replay or per-item initialization |
| Bound checkpoints and ordinary recovery | Normal initial/latest checkpoints and same-arm live restore | Exact saved state and valid followup; elapsed input remains unchanged |
| Actual fresh-slot pairing | Empty new bank, existing relocation/sync helpers | Hash/inode checks, conflict rejection; no old bank import |
| Physical quality allocation | Existing observer plus explicitly labelled fixed bins | Same model/Adam/RNG/events with observer; no extra forward passes |
| Bounded storage | Publication/removal hooks, shared inode identities, between-arm reconciliation | Release charges only after last-link deletion; compare final actual bytes |

Before the scientific run, use a **five-update / 600-second** isolated cold-start
readiness check: four updates from zero and a one-update same-arm live restore.
Its generation namespace is separate (**718400000**); its learned state and input
bank cannot initialize the study. Reuse existing contract tests and exact-input
CUDA proof where unchanged. Run the smallest affected proof for the diagnostic
bin/schema changes. Do not require equal future trajectories under unequal elapsed
policy inputs or add exact historical replay to normal recovery.

The original attempt (superseded by the correction below) had hard limits of **21,600 seconds (six hours)** including
setup/evaluation/finalization, **1,408 updates**, **12 GiB added artifacts**, unchanged
**48 GiB process-tree RSS** and **16 GiB disk reserve**. This is a new registered
budget for a new initialization experiment, not an extension of the stopped warm
run. Earlier integrated timings around 10–15 seconds/update are engineering
forecast evidence only; allocate room for new input generation and finalization.

Replace cumulative charges for deleted single-link publication files with a
conservative current ownership ledger. Existing publication/removal paths supply
exact file identities; only release a charge after successful deletion of its
last link. Keep shared files charged while any link remains. Reconcile between
arms and report final actual unique owned bytes. Do not scan trees or save the
ledger per update; keep state saves at normal checkpoint cadence and ledger saves
at arm boundaries. This avoids exhausting a storage budget on already deleted
files while preserving the real limit.

## Input-separation correction after the first cold attempt

The first cold attempt stopped at baseline update 128: four development geometries
(five training slots) and eight confirmation geometries (22 slots) overlapped
actual fresh training inputs. Different generation seeds can create identical
physical geometry; rate differences do not change the existing geometry identity.
All its quality outcomes are invalid as held-out evidence and excluded from
selection. Preserve its checkpoint and leakage closeout; no learned state, input
bank, rankings or panels initialize the replacement. Panels now exclude all five
prior panels (original, warm development/confirmation, failed cold development/
confirmation). Only engineering cost and identity evidence may be reused.

Before creating a training bank, derive a canonical reservation containing the
sorted union of the new development and confirmation geometry hashes, bound to
both complete panel files. Training input configuration carries this reservation
under `geometry_reservation` with schema `blueprint-geometry-reservation-v1`.
Diagnostic readiness reserves its own panel. The normal input generator validates
this optional contract once and constructs an in-memory set. Existing users with
no reservation retain their behavior. Every bank manifest includes the reservation
when supplied; incompatible unreserved or differently reserved banks are rejected.

After a native-valid generation and before blueprint editing, admission or any
policy action, reject reserved geometry and continue the existing attempt loop.
Do not increase the eight-attempt or 180-second slot allowance. Keep the original
generation status and evidence, adding an explicit reservation rejection flag to
each attempt. Loader validation checks the flag against the bound set, permits
accepted prefixes only when reserved, and rejects any reserved final admission.
The canonical slot/seed mapping and paired immutable input reuse remain unchanged.
Inherited starts can change geometry because they materialize their own parent's
edited best layout. `Runtime.episode_runtime` therefore checks the already-computed
actual metadata geometry against a cached reserved set before opening/restoring
any episode or returning its input. A reserved inherited start fails closed with
an explicit diagnostic, retaining partial evidence and no winner. Do not change
donor eligibility, quotas, sampling or fallback behavior. Include the restore
path and a fixture with an unreserved root but reserved edited parent. Keep the
existing actual-input overlap check at closeout.
Count reservation rejection costs from existing attempt metadata at arm boundaries;
no extra hot-loop journal, scans, hashes, model calls or learning changes.

Readiness must demonstrate actual reserved-input rejection before training, no
reserved final input, paired reuse and valid live restore/followup. Its panel and
training namespace intentionally coincide, making first-attempt reservations
observable without adding training updates. Retain five diagnostic updates and
600 seconds; source-bind a new receipt. Reuse unchanged CUDA kernel proof.

The replacement still allocates 1,408 updates to the same profiles, endpoints and
selection rule. Record the failed 128 separately: at most **1,536 total cold
scientific updates**, of which 128 are discarded. Preserve the original absolute
six-hour deadline: **1789374130.8190546 Unix seconds**, derived from the first
attempt start 1789352530.8190546 + 21600. Replacement wall allowance is the lesser
of 18,000 seconds and time remaining until that deadline, including its setup,
evaluation and finalization. The original 12 GiB added-artifact cap includes both
cold attempts and the replacement readiness; carry prior owned bytes as a fixed
initial charge. RSS/disk reserve caps remain unchanged. No retry or extension
following another scientific failure. Use a new frozen source/output directory.

Test expired and shortened absolute deadlines and persistence of prior owned-byte
charges through child budget reloads. The correction above supersedes the original
attempt allocation; it does not create a second six-hour allowance.

## Corrected execution disposition

The corrected baseline stopped at update 86 when the actual inherited-input guard
found a reserved development geometry. This was a required fail-closed outcome;
there was no committed training-input overlap. All workers exited and main stayed
paused. The requested comparison remains incomplete. Its retained evidence and
214-update cumulative cold accounting are documented in the
[study report](../reports/2026-09-14-cold-start-quality-replay-study.md).
Changing reserved-donor eligibility is a pending design decision, not an authorized
extension or automatic restart under this protocol.

## User-authorized collision replacement and own-arm continuation

On 2026-09-14 the user rejected stopping the whole comparison for an occasional
geometry collision and authorized replacing a colliding input. Geometry equality
alone is not equivalent to importing learned model state: the current identity
excludes rates/utilizations. Retain the reserved-start precaution, but resolve
expected generated/inherited collisions through bounded input replacement.

Keep the existing donor sampling and slot shuffle exactly. After selection, inspect
only selected improving inherited layouts when a reservation is configured. Replace
a reserved selected slot with a fresh slot, preserving stream, wave and requested
depth; record `declarative.curriculum-reserved-geometry` and the rejected parent
identity. Update actual depth counts while retaining original complete/improving
parent counts and requested quotas. Do not resample all donors, alter any unreserved
slot or weaken native validity. Fresh replacements use the existing canonical
slot bank and eight-attempt/180-second bounds. The actual episode/restore guard
remains a defensive invariant. With no collision, the plan is identical; with no
reservation, normal execution performs no additional geometry work.

Continue the same seed-71 baseline from its own valid update-86 checkpoint. It
started from random initialization; this is ordinary own-arm continuation, not a
trained initializer for another profile. Reuse its fixed panels, actual fresh files,
initial evaluation, 0/32/64 evaluations and learning records. Only the first cold
attempt's 128 updates remain discarded. The study still contains 1,408 scientific
updates, including the retained 86, and therefore needs **1,322 new updates**.
No partial quality observation changes a profile, seed, endpoint or selection rule.
All other ten training arms start from their registered empty state.

The replacement rule is exactly prefix-preserving: previously admitted inherited
starts were all unreserved, as checked by the existing episode guard and retained
1,376 actual input rows. Only the pending wave-44 stream-5 slot changes. Confirm
this once from actual input/plan identities, without native replay. Do not import
an arbitrary checkpoint or another profile; the protocol binds the specific
stopped study, launch, origin and update-86 checkpoint.

Use a new source/output root and preserve the previous frozen source and evidence.
Perform one atomic, explicit own-arm source amendment using existing checkpoint,
input relocation and immutable hardlink storage. Verify the old source against its
original root, and permit only the reviewed curriculum/research-orchestration and
test source changes; native, learner, optimizer, replay, data and numerical settings
must match. Hardlink only immutable publication/input files listed by their existing ledgers;
create new mutable controls/logs/tracking state, rebase the ownership ledger,
and retain immutable original input references. Rewrite only the checkpoint's
launch identity and the pending changed `next_start`; prove model, Adam, RNG,
buffer, ordinal and all other stream fields unchanged. Retain the old launch and
checkpoint bindings in a continuation receipt; preserve the original empty-start
proof. Publish the complete amended directory atomically, with no partial runnable
state on failure. Historical zero/evaluation evidence retains its original binding.

Regenerate only the pending plan from its retained selected-slot materializations;
apply the same replacement helper used by normal planning and write a new plan
version. Do not replay learning or resample historical donors. Preserve all committed
input files, plus complete pre-generated future slots. Quarantine/omit only incomplete
uncommitted generation folders in the new copy, retaining their originals and a
bounded amendment diagnostic. No committed input is redrawn. Synchronize these
immutable inputs into the shared canonical bank before subsequent fresh arms.

A new source-bound five-update readiness (0→4 plus live 1→2) validates normal
execution and recovery. Focused real-native tests exercise selected-parent collision,
exact preservation of other slots and the amendment's empty native boundary. Reuse
unchanged CUDA kernel evidence. Validate the amendment before launching its actual
86→128 continuation; that continuation counts only 42 new training updates.

Keep the original absolute deadline 1789374130.8190546 and remaining resource caps.
Count the retained 86 once, not as both discarded and replayed work. Charge prior
actual artifact ownership once; cloned immutable hardlink versions are excluded
from new charges using identities collected during copying, while rewritten/new
files are charged normally. No extra tree scan, snapshot journal or per-update
persistence is introduced. Report previous and new execution costs separately.
This user-authorized continuation supersedes the earlier no-retry disposition;
it does not authorize model transfer, automatic adoption or restarting main.

Verify original launch, job and checkpoint hashes are unchanged after amendment
success/failure and actual continuation; do not copy old console/worker/W&B files.

## User-authorized one-hour clock extension

On 2026-09-14 the user explicitly authorized up to one additional hour to complete
the full comparison. The new hard deadline is **1789377730.8190546 Unix seconds**
(18:22:10.819 KST), exactly 3,600 seconds after the previous deadline. Scientific
updates, profiles, selection, initializations, panels, native limits and storage/RSS/
reserve caps stay fixed. This supersedes only the previous wall-clock ceiling.

Use an operational completion command, `operations/finish_cold_quality_study.py`,
to finish the remaining two seed-113 confirmation arms. Keep the frozen learner,
original protocol and readiness source intact. Bind this additional controller
separately in an immutable clock-extension receipt, alongside the unchanged source
and original receipt. It imports the existing cold preparation, child execution,
summary, allocation, pairing, resource accounting and statistical analysis functions.
No learner, launch or checkpoint amendment is needed.

After the first seed-113 baseline starts, suspend only the study coordinator; its
child keeps training with its original guard. Wait for its native-valid update-128
completion and child exit, then terminate the suspended coordinator. Preserve its
last study receipt before publishing the extension receipt. The completion command
requires exactly eight completed development arms, their immutable selection, all
three read-only seed-71 confirmation receipts, and the completed seed-113 baseline.
It summarizes that baseline once and adds its 128 updates exactly once. It then
creates priority-8 and uniform-16 through the original empty initializer and shared
fresh bank, reusing the baseline initial confirmation evaluation as before.

The controller continues the same storage ledger and output directory. Its wall
guard uses time remaining to the new absolute deadline; each new branch receives
that bounded remainder. Normal source/family/readiness checks remain active. The
additional controller authenticates its own file, the original registered source,
prior receipt, selection and carried endpoints at entry and completion. Errors
retain partial results with exact accounting and no final winner. The deadline
change cannot alter an endpoint, choose a new candidate or import learned state.

Continuity checks:

| Existing benefit | Extension approach | Check |
| --- | --- | --- |
| Cold paired initialization and fresh inputs | Reuse original prepare/bank/cache | Same seed-113 initial model, empty proof, canonical slots |
| Optimized CUDA learner and persistent workers | Call unchanged original branch CLI | Original full source/readiness bindings and actual remaining arms |
| Completed scientific work and immutable selection | Carry complete receipts; summarize finished baseline once | Exactly 1,408 final scientific updates; checkpoint/selection hashes unchanged |
| Bounded storage and recovery | Continue existing ledger, no duplicate snapshots | Existing removal hooks and one boundary reconciliation |
| Resource bounds | One explicit additional hour; unchanged other caps | Reject expired/wrong deadline; outer guard includes finalization |

Verify the controller's handoff prerequisites, exact update accounting, initial
cache/model pairing, immutable bindings and hard-deadline rejection with focused
checks. Reuse the unchanged CUDA/readiness/Rust evidence; no repeated training or
new kernel readiness experiment is justified by an orchestration-only time change.
