# Declarative learner: retained optimizations and short training

Status: complete. The interrupted fresh-only screen is preserved, the 80/10/10
curriculum is restored, and the bounded seed-17 continuation finished. The earlier random action screen established
contract preservation and examples of useful edits; it did not estimate the
probability of learning large-factory improvements.

## Scope and continuity

The [stage design](../designs/2026-09-13-declarative-learning-continuity.md) records
the predecessor requirements, their new implementation and the verification
boundary. The new explicit `declarative_learning.py` command uses joint facility
position/rotation/port edits and row/column insertion or deletion. Routing,
validation and retention of the best are automatic native operations. The
original directed production blueprint remains fixed. Normal construction and
the isolated exact solver are unaffected.

The policy uses a packed graph encoder and a root action decoder, followed by
conditional compatible port choices without replacement. One environment action
has one joint log probability, used consistently for sampling, PPO and success
imitation. This removes the need for the policy to issue separate routing and
commit actions. It also avoids enumerating every complete port assignment.
Old actor weights and labels are not reinterpreted under the new schema.

The shared execution kernel retains persistent native workers, ordered central
sampling, CUDA updates, a CPU behavior copy, packed graph operations, cached
replay inputs, minibatch preparation ahead of GPU work and separate RNG streams.
The graph/message-passing model, continuing GAE, centered advantages, separate
actor/critic Adam ownership, KL actor stopping with continued critic training,
positive-advantage imitation and stage weighting remain. Existing uniform and
strict balanced sampling remain available. Fresh training explicitly uses
`balanced_available`: four sequences per size stratum when both exist, otherwise
eight from the available stratum, and no replay before any success is discovered.

Successful replay requires a native-validated area improvement over the episode's
retained best. Each entry contains the changed preparation actions since the
preceding area improvement, including temporarily overlapping or pending drafts.
Rejected and unchanged actions are omitted from the imitation labels. A valid
non-improvement does not erase preparation credit. Only tile/turn improvement
with unchanged area gives no positive area reward and does not independently
create a success entry. Entries receive returns through the actual episode
terminal and become eligible only in a following update. One factory episode can
produce multiple entries; the entry count is not a count of distinct factories.
Imitation applies positive weight only when the recorded return exceeds the
current detached value prediction.

Thirteen isolated server tests passed, including the retained optimizer's CPU
numerical comparison, CUDA/nonempty-Adam replay, actor-safe KL tail, joint
likelihood, real catalog port geometry, rotation/port editing, preparation
credit across chunks and restoration, and positive CUDA imitation. A four-update
integrated run restored eight unfinished 32-action native drafts with their
original generated inputs and exactly replayed an update with nonempty Adam.
The explicit positive-trajectory fixture was confined to verification.

## Completed fresh-only seed and corrected continuation

The original `declarative-learning-short.json` registered seeds 17/29/41 with
128 updates each. Seed 17 completed all 32,768 actions and 512 64-action episodes
in 1,251.195 seconds. It used 512 actual generated starts, with 467 distinct
geometries. Seed 29 reached published update 22 before the matrix was terminated;
seed 41 did not run. Do not report the planned three-seed totals as completed.

The user identified the omitted 80/10/10 best-state start curriculum. This was
an established requirement, missed in the earlier blueprint migration and in the
initial continuity review. Successful-experience imitation does not replace
exposure to policy-transformed initial geometries. The user then explicitly
changed donor eligibility to require strict area improvement in the preceding
pass, while retaining uniform sampling among eligible donors.

`declarative-curriculum-short.json` registers one continuation: seed 17 from
update 128 through 160, adding 8,192 actions and 128 episodes. Both Adam states,
sampling/shuffle RNGs and 2,232 successful replay entries are imported exactly.
A separate v2 launch records the immutable v1 parent; it does not relabel the
fresh-only prefix as curriculum training. New generated slots use a separate
store and the continuing global slot/seed sequence.

One curriculum wave is eight completed 64-action episodes, spanning two PPO
updates. Depth one starts from a strictly improved fresh parent's validated best;
depth two requires another strict improvement in that depth-one episode. The
parent must come from the immediately preceding wave. Copy its actual request
and complete validated best, preserve root/parent identity, and independently
collect current-policy actions from that new start. New rewards are normalized
by the new start area; cumulative root-relative reductions are separate metrics.

Fractional quotas retain 10% targets over time despite eight slots per wave.
Requested and actual depth counts, eligible pools and fresh substitutions are
reported separately. No replacement, outcome ranking or evaluation donors are
used. A failed improvement is not an eligible parent. Missing/corrupt selected
inputs fail explicitly, rather than generating replacements during restoration.

All numerical PPO/SIL/native settings remain: eight streams, 32-action chunks,
width 40, two message rounds, centered GAE, gamma 1, lambda .95, Adam .0003,
four epochs, minibatch 64, clip .2, KL .02, entropy .001, gradient clipping .5,
SIL coefficient .05 and eight draws per active actor minibatch. Native allowances
are 50,000 routing expansions per action, 3,200,000 per episode and 300 seconds.

The frozen evaluation panel contains sixteen small and sixteen large distinct
geometries, selected from forty generated slots without editing-outcome selection.
Small products are watered-grass/copper-liquid; large products are food/medical.
Every evaluation uses two samples per factory, 64 actions and ten seconds.
The continuation reuses the identical update-128 model evaluation and original
random baseline, then evaluates updates 144 and 160. Samples from one factory
are correlated; this single-seed continuation does not isolate a curriculum
causal effect or establish convergence.

Five small evaluation geometries also occur in the original 512 training inputs.
All sixteen large evaluation geometries are disjoint. Therefore report the full
panel and the 11-small/16-large disjoint subset separately. Different rates alone
do not establish a different geometry. The previous editor's two-second budget
and different action contract are not direct score/throughput comparators.

## Execution measurement and normal launch

Each measurement cycle performs four updates (1,024 training decisions and
sixteen complete episodes), durable publication/retention and thirty-two
single-sample evaluation episodes through the same implementations as training.
Every mode/repetition uses the same sixteen actual generated training inputs.
There is one warmup and three measured cycles per mode.

| Execution | Median cycle, s | Measured range, s |
| --- | ---: | ---: |
| Serial CPU reference | 30.780 | 30.240–30.841 |
| Eight workers, CPU learner | 26.133 | 26.075–26.143 |
| Eight workers, CUDA learner | 21.850 | 21.747–21.856 |

The CUDA path is 1.409 times as fast as the serial CPU reference, with native
steps overlapping on all eight workers. The predecessor's 1.5 ratio is not
preserved quantitatively on this changed workload. An initial summary assertion
incorrectly treated that comparison as a launch gate and prevented the final
receipt from being written after all twelve cycles had completed. Removing this
reporting-only assertion allowed reconstruction from the completed test,
continuity, profiler and cycle records; no cycles were rerun. The receipt records
the recovery script hash and the exact source-change scope.

All diagnostic processes and profiling ended before the normal launch at 14:35
KST. Main training uses normal execution settings, lightweight metrics and the
existing resource guards. Source authentication happens at launch; it is not
repeated inside every update. The initial measured forecast was 2,126.6 seconds.
The continuation below distinguishes collection, optimizer, publication and
evaluation costs; early cycles did not represent restored replay hydration.

## What the learned policy does

These are saved evaluation records, without additional rollout or optimization.
Mean area reductions are factory averages over two samples each:

| Update | Small, full 16 | Small, disjoint 11 | Large, disjoint 16 |
| --- | ---: | ---: | ---: |
| 0 | 4.617% | 5.226% | 0.215% |
| 64 | 27.471% | 28.509% | 8.970% |
| 128 | 30.715% | 33.272% | 19.759% |

On large factories, deletion action share rises from 32.18% to 71.00% to 91.65%.
Placement share falls from 35.45% to 10.64% to 2.39%. The fraction of steps with
facility overlap falls from 95.36% to 46.34% to 11.67%, and pending connections
from 96.97% to 59.38% to 30.71%. The policy increasingly avoids destructive
unfinished drafts and compresses rows/columns. However, final action rejection
share is 68.16%, up from 33.40%. Better outcomes coexist with many failed attempts.

The learned large-factory reduction accumulates from 4.118% after eight actions
to 13.830% after 32 and 19.759% after 64. The untrained large-factory policy stays
at 0.215% across those milestones. This shows useful successive edits, while
not establishing broad facility rearrangement or planning ability.

A fixed contiguous sample of eight training episodes in chunks 63/64 contains
34 successful segments: 32 end in a band deletion and two in placement. Some
include earlier insertion or movement. That is descriptive attribution, not
proof that each preparation action was causally necessary. The sample contains
five small and three medical factories; it is not a representative food study.
The analyzer explicitly separates collection policies from evaluation checkpoints.

The user's concern about regular spaced-grid generator outputs is consistent
with this deletion-heavy behavior, but the observations alone do not prove that
generator regularity causes it. Inherited starts restore required exposure to
transformed layouts; generator spatial diversity is a separate local experiment.

## Curriculum verification and results

Four changed-contract tests pass locally: fractional quota accounting and stable
selection, strict success filtering, one-inflate-per-source-chunk restored replay
hydration, and exact native inherited admission/restoration with both depth
lineages and missing/corrupt input rejection. Eight declarative learner regression
tests also pass. Existing 13-test CUDA/optimizer and full workspace evidence is
reused because the optimizer/model/native semantics are unchanged.

The six-update server continuation diagnostic uses separate generated inputs and
the actual trained update-128 parent. It imports weights/Adam/RNG/replay exactly
and exercises eight workers, next-wave publication and live native restoration.
An initial verification assertion compared restored elapsed time with a later
live snapshot, rather than the committed checkpoint. Corrected the comparison
and resumed after the three already-published updates, without repeating them.
The six-update diagnostic passed in 388.257 seconds, including both restored
replay cache starts. No profiler or repeated benchmark matrix was used. The
server verification processes ended before normal training started.

The normal continuation completed all 32 updates, 8,192 actions and 128 episodes
in 547.332 seconds (543.843 seconds inside the learner arm). All 32 updates had
positive summed area reward, and eight native workers overlapped. Actor training
ran four epochs in every update. Replay grew from 2,232 to 3,166 entries, with
2,048 draws per size stratum and 3,432 positive imitation labels of 6,649 labels.
These are segments/labels, not independent successful factories.

| Start depth | Requested slots | Actual starts | Fill | Further-improved episodes | Mean further area reduction |
| --- | ---: | ---: | ---: | ---: | ---: |
| Fresh | 102 | 109 | Includes seven substitutions | 91 / 109 | 27.937% |
| One prior improvement | 13 | 13 | 100% | 6 / 13 | 4.604% |
| Two prior improvements | 13 | 6 | 46.15% | 4 / 6 | 7.569% |

The actual mix is 85.16% / 10.16% / 4.69%. Seven unfilled inherited slots used
fresh factories. Depth-two shortage includes the first migration wave's lack
of depth-one parents and later unsuccessful parent passes. Counting requested
slots as actual 10% participation would be incorrect. Inherited groups' mean
root-relative reductions are 49.988% and 49.756%; these are selected, different
cohorts and must not be read as an unbiased three-pass improvement curve.

The continuation generated 109 new fresh starts in 110 attempts, with one
rejected generation attempt; nineteen starts used exact inherited bests. The
combined parent/child ledger has 640 episode slots and 579 distinct geometries.
The same five small panel geometries overlap; all large panel geometries remain
disjoint. Every evaluation output is native validated.

| Evaluation update | Small, full 16 | Small, disjoint 11 | Large, disjoint 16 |
| --- | ---: | ---: | ---: |
| 128, reused identical parent model | 30.715% | 33.272% | 19.759% |
| 144 | 34.851% | 36.232% | 19.911% |
| 160 | 34.977% | 36.458% | 20.008% |

The continuation is not a controlled curriculum-effect comparison. It does
show additional improvements from inherited compact starts while preserving
fresh-input performance under this bounded run. It does not establish extreme
compression, convergence or generalization to arbitrary hand-built factories.

Timed collection was 139.038 seconds, optimizer execution 32.293 seconds and
publication 106.677 seconds. Worker minibatch preparation (12.452 seconds) overlaps
optimizer execution and must not be added to it. New evaluations took 17.801
seconds; the imported update-128 evaluation is excluded. These phase timers do
not cover all cache hydration, bookkeeping and process startup. The first normal
update appeared at 176.046 seconds, dominated by one-time restoration of prior
successful-experience inputs; later updates were substantially shorter.
Process-tree peak RSS was 16.309 GB, CUDA peak allocation 330.464 MB and retained
arm files 1.746 GB, within all registered ceilings. No diagnostic mode ran inside
main updates or completion reporting.

The post-run summary was corrected to mark the imported evaluation as reused
and exclude its historical time. The frozen training checkout was unchanged;
the corrected reporting module read only compact saved records from a separate
path. Its hash is recorded with the final report. No rollout was repeated.

## Final scope decision

After the local spatial-diversity probe, the user clarified the actual deployment
workflow: generate a complete factory with this constructor, then repeatedly
optimize that concrete layout with the model. Training inputs should therefore
cover constructor outputs and policy-transformed states. Human-like arbitrary
input layouts are not a required target distribution for this workflow. Keep
the current generator and restored 80/10/10 starts; do not adopt the local
perturbation alternatives or launch a new constructor redesign in this slice.
Products, rates, sizes, connectivity and progress of compression remain relevant
coverage dimensions. Further long training or a batch-size comparison is not
part of this completed short continuation.

## Reproduction

Use the repository's existing Python environment with PyTorch/CUDA and the
release `aic-pilot-native` extension on the Linux worker host. Paths below are
relative to a source checkout with the matching runtime files.

```sh
# PARENT is the retained fresh-only launch; PANEL is its frozen panel.json.
python tools/factory-edit-pilot/declarative_learning.py verify \
  --native-library target/release/libaic_pilot_native.so \
  --panel "$PANEL" --parent "$PARENT" \
  --output artifacts/declarative-learning/curriculum-verification
python tools/factory-edit-pilot/declarative_learning.py prepare \
  --native-library target/release/libaic_pilot_native.so \
  --panel "$PANEL" --parent "$PARENT" \
  --verification artifacts/declarative-learning/curriculum-verification/verification.json \
  --output artifacts/declarative-learning/curriculum-short
python tools/factory-edit-pilot/declarative_learning.py run \
  --output artifacts/declarative-learning/curriculum-short --environment .tracking.env
python tools/factory-edit-pilot/declarative_learning.py report \
  --output artifacts/declarative-learning/curriculum-short
```

`run` also restores an interrupted launch from its latest complete published
checkpoint with the same source/configuration identity. It restores both
optimizers, all sampling/shuffle RNGs, replay and unfinished native episodes.
Retained checkpoint dependencies include their source chunks; older chunks
needed for restoration or success replay are not deleted by routine retention.
The reporting command consumes compact records without additional rollouts or
source-episode replay.


## Retained evidence

The frozen parent remains on `ashrimp-arch` under
`/home/ashrimp/Srv/aic-declarative-learning-short-20260913`; the child source and
run are under `/home/ashrimp/Srv/aic-declarative-curriculum-20260913`, with the run
at `artifacts/declarative-learning/curriculum-short-v1`. The final checkpoint is
`seed-17/checkpoints/0160.pt.gz`; all 160 source chunks required by its replay,
actual parent/child inputs and inherited start files remain retained. Selected
local reports, receipts and the checkpoint are under ignored
`artifacts/declarative-learning/curriculum-completed/`. The checkpoint copy alone
is not a portable restore bundle. The compact provenance manifest records the
launch, verification, report, native and checkpoint identities.
