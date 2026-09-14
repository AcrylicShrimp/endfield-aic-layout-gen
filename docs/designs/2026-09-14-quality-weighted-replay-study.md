# Quality-weighted successful-experience learning study

Status: accepted; learning protocol passed round 2 and the time-aware readiness
correction passed independent round 3 review after actual execution evidence.

## Purpose and discarded evidence

The user rejected the previous experiment and requested a new design and new
server experiments. Its scientific findings, shortlist and baseline recommendation
are withdrawn from this study. Do not use the old arrival corpus, frozen utility
cache, panel outcomes, native-audit verdict or exposure results to choose arms,
thresholds or a winner. Preserve historical files as marked withdrawn records;
do not destroy actual learner checkpoints or input evidence. Existing code,
contract tests and measured execution techniques remain reusable infrastructure.

The question is whether assigning more learning to better successful edits makes
the trained policy produce better validated layouts. At-least-once replay coverage
cannot answer it. A high-coverage ceiling never excludes an arm from training.

## Quality, allocation and learning outcomes

Keep three distinct measurements:

1. Outcome quality of a success: actual incremental validated area reduction
   divided by the retained-best area immediately before that segment, using the
   existing feature extractor and complete native witness. Report continuous
   quality and its distribution separately by size and facility-edit/deletion
   category. Rarity, geometry heuristic and positive advantage are candidate
   prioritization signals, not definitions or proofs of physical quality.
2. Allocation: draws, mean native quality per draw, quality-weighted draw mass,
   actual stage-balanced raw SIL contribution, correction-weighted contribution,
   zero/positive label weights and sampled sequence lengths. Compare draws with
   the eligible pool in the same update/stratum/category. Freeze descriptive
   quality quartile boundaries from the new initial pool; ties/empty bins stay
   explicit and never become a minimum-count adoption gate. Record terminal
   attempts before eviction so admission summaries are not survivor-only.
3. Learning: mean actual area reduction on fixed independent evaluation factories
   after equal numbers of new on-policy updates. Average action-seed replicates
   per geometry, then factories per stratum, then the two strata equally. Also
   report each stratum, paired geometry differences, win/tie/loss counts, valid
   retained output, native guard outcomes, runtime and retained disk usage.

A larger mean raw SIL loss is not itself a better trained policy. A score being
high is not circular evidence that a success is physically good. Keep the reward,
terminal returns, stage normalization and SIL coefficient unchanged.

## Fixed development arms

All start from one newly paused, complete-wave main checkpoint. No rejected
old experiment checkpoint is a training parent. Keep native semantics, generator,
80/10/10 curriculum, optimizer and model unchanged. Compare these eight profiles:

| ID | Retention | Replay sampling | Sequences per minibatch | Purpose |
| --- | --- | --- | ---: | --- |
| baseline-8 | Current v1 | Balanced uniform | 8 | Control |
| uniform-16 | Current v1 | Balanced uniform | 16 | Quantity contrast |
| uniform-32 | Current v1 | Balanced uniform | 32 | Quantity contrast |
| gain-8 | Current v1 | 50% uniform / 50% native-gain score | 8 | Physical quality alone |
| score-8 | Current v1 | Existing rare-3pp score mixture | 8 | Proposed rarity/quality score |
| geometry-8 | Current v1 | Existing capped-exterior score mixture | 8 | Incremental geometry hypothesis |
| priority-8 | Current v1 | Existing positive-advantage reference | 8 | Learning-contribution reference |
| protected-score-8 | v2 q=0.10, grace=64, area ordering | rare-3pp mixture | 8 | Retention contrast to score-8 |

The gain-only profile uses the existing bounded score-mixture formula and
normalization, with native gain as its sole score. No weights are tuned. Existing
rare/geometry/reference constants remain declared hypotheses rather than previously
validated winners. Capacity stays 4,096 sequences / 262,144 labels for all arms.

## Development and confirmation allocation

Generate two new, disjoint 32-factory panels, 16 per stratum, using separate fixed
seed namespaces and the established external-data generator. Development panel
generation/action seed bases are 714200000/714300000; confirmation uses
715200000/715300000. Replicate-two action/shuffle seeds are 714400029/714500029
for the retained learner seed 29. Bootstrap seed is 714600000. Neither panel is
selected by editing outcomes. Each evaluation uses two fixed action-seed
replicates and the current 64-action/10-second/routing limits. The confirmation
panel must not be evaluated before development selection is written immutably.
Reject overlap with retained parent training identities or actual new training
identities; record all generation rejections. Do not silently drop overlapped
factories from a final mean. Development and confirmation panels are disjoint
from each other and from prior experiment panels.

Development: run all eight profiles for 32 actual optimizer updates from the same
parent, then evaluate their endpoints on the new development panel. Every arm
gets this learning test; no coverage, native-audit score or proxy improvement gate
can prevent it. Select the two nonbaseline profiles with the highest development
mean validated area reduction. Exact ties (absolute tolerance 1e-12 percentage
points) prefer fewer replay draws, fewer changed mechanisms, then stable ID.
Select two even if neither beats baseline; confirmation can establish a negative
result. Preserve the immutable selection receipt before any confirmation scores.

Confirmation: continue the baseline and those two selected development branches
for 64 more updates, ending at parent+96. Run a second stochastic training
replicate of each of the same three profiles for 96 updates from the original
parent. This gives two training replicates per finalist and 736 newly executed
updates total (256 development + 192 continuation + 288 second replicate).
Replicate one preserves the original sampler/shuffle RNG states and learner seed.
Replicate two resets only the behavior-action sampler and PPO shuffle RNGs to
fixed experiment seeds, paired identically across its three arms. It is an
explicit stochastic branch, not an exact continuation. Keep learner seed/episode
namespace, actual fresh slot identities, model and Adam states unchanged.
Replay RNG resets retain production learner-seed/global-update behavior in both
replicates. State these are correlated single-parent replicates, not independent
training seeds or evidence across all initializations.

Fresh input files are paired where applicable through one append-only experiment
bank, linked into each private child bank. Synchronize newly generated immutable
fresh slots after each sequential arm, rejecting conflicting existing content;
never regenerate an available bound slot. Inherited actual starts may differ as
learning changes. Retain and disclose their true curriculum plans; do not impose
one arm's learned starts on another. Run arms sequentially to avoid competition.

## Final recommendation

The primary result is actual confirmation-panel validated area reduction, averaged
across the two training replicates using the geometry/stratum aggregation above.
Rank the baseline and two finalists by this result; report exact paired differences,
per-stratum tradeoffs, both replicate outcomes and runtime/storage alongside it.
The best observed setup is provisional when differences are inconsistent or
uncertain. It must not be reported as globally optimal or conclusively superior.
Use paired bootstrap intervals over whole geometries, stratified by size, with
both training/action replicates kept attached to each geometry (10,000 resamples,
fixed seed). These quantify panel variability conditional on this parent and
observed training replicates, not independent-training-seed uncertainty.

Hard invalidity, missing required data, evaluation leakage or an unfinished budget
prevents a final winner and yields incomplete evidence. Coverage saturation,
a missed arbitrary percentage-point gain, or source-byte growth below the actual
resource cap does not disqualify a completed arm. Report real cost/quality tradeoffs;
choose a cheaper arm only on an actual equality tie, not an invented tolerance.
There is no automatic deployment or main-run restart. Distinguish best observed
quality from a confident adoption recommendation. If both candidates are worse,
that supplies an actual learning-based reason to retain baseline.

## Execution continuity and bounds

Pause the current main run gracefully immediately before the first new server
experiment; bind the newly completed wave and keep main paused afterward. Freeze
new experiment source before preparing runs. Use explicit isolated policy
transitions, batched metadata backfill, current packed likelihood/sampler/index,
persistent workers, CUDA learning, compact publication and ordinary v2 restore.
Initial enabling of priority sampling may bootstrap from the new main parent;
same-profile continuation/live restoration must preserve checkpointed priorities,
utility and observation timestamps without recomputation. Native-quality metadata
is filled once at import or true terminal admission and reused across restores.
Bind new operations/helpers/configuration explicitly alongside normal sources.
No diagnostic observer or audit becomes part of ordinary main training.

| Existing benefit | Reuse | Check |
| --- | --- | --- |
| True terminal credit and semantic identity | Existing Buffer admission/observer and retained labels | Hand-controlled terminal and duplicate tests; unchanged identity/returns at fork |
| Cached compact replay and batched source loads | One boundary backfill; reuse source chunks and packed cache | Load counts and measured setup cost; no historical rollout replay |
| Packed CUDA PPO/SIL and persistent workers | Normal run/learner/pool | Actual CLI progress and phase timings for every arm |
| Exact saved-state live-draft recovery | Normal checkpoints and own-arm one-update restore check | Exact restored model/Adam/RNG/buffer/native/input state, monotonic elapsed allowance and valid followup; separate identical-input CUDA learner replay |
| Input and curriculum continuity | Actual immutable fresh slot files/private inherited plans | Binding/overlap tests and actual ledger audit |
| Bounded storage without trajectory dumps | Normal publication/retention plus compact quality aggregates | Actual added bytes, required roots and source pruning checks |

Readiness is limited to five new updates and 600 seconds. The full learning study
is limited to 736 new updates, 14,400 seconds and 12 GiB of added artifacts, with
unchanged 48 GiB process-tree RSS and 16 GiB disk reserve. Count setup, evaluation
and finalization in the study wall cap. Native limits are unchanged. Stop cleanly
and retain the latest checkpoint at a hard limit; do not claim completion for
partial arms or extend runs after inspecting outcomes. Report discarded historical
results separately from all newly generated scientific evidence.

The live elapsed fraction is an existing policy input, not merely diagnostic
bookkeeping. Restoring a live checkpoint preserves its state and remaining native
allowance; subsequent wall-clock observations can differ from uninterrupted
execution. Do not require identical future trajectories under unequal actual
inputs, or change the time feature to manufacture equality. Reuse the five
executed readiness updates, preserve the original failed equality receipt, and
check exact saved-state restoration plus valid followup and exact CUDA transitions
on identical recorded native fixture inputs. This corrects readiness evidence
without changing any of the eight scientific arms or their allocation.
