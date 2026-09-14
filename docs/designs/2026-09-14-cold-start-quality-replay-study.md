# From-scratch quality-weighted replay comparison

Status: accepted after independent proposal review (round 2, PASS_WITH_NOTES). The user explicitly replaced the
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
**716900000**, keeping the existing canonical slot mapping and generation contract.
Profiles and initialization seeds pair actual fresh files by global slot. Import
and synchronize only immutable generated files, reject conflicting content and
preserve each arm's own inherited starts. Do not reuse any historical training
bank as the new bank's source.

Generate two new 32-factory panels, 16 per size stratum, with two action replicates
per geometry. Development generation/action seed bases are **716200000/716300000**;
confirmation uses **717200000/717300000**. Each panel allows 128 generation attempts
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
Use 10,000 whole-geometry stratified bootstrap resamples, seed **716600000**, with
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
Its generation namespace is separate (**716400000**); its learned state and input
bank cannot initialize the study. Reuse existing contract tests and exact-input
CUDA proof where unchanged. Run the smallest affected proof for the diagnostic
bin/schema changes. Do not require equal future trajectories under unequal elapsed
policy inputs or add exact historical replay to normal recovery.

The new scientific run has hard limits of **21,600 seconds (six hours)** including
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
