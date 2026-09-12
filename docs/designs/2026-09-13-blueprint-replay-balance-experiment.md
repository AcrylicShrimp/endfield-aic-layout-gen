# Separate success exposure, replay balance and imitation strength

Status: the explicit runtime and complete-cycle readiness are verified. Main
training requires the frozen launch manifest. This remains the registered bounded
research slice; see the [runtime report](../reports/2026-09-13-blueprint-replay-runtime.md).

## Question and evidence

Can the centered blueprint policy learn to finish useful large-factory edits
when successful examples are available and receive sufficient training weight?
Distinguish supplying those examples, changing replay composition, and changing
the overall imitation coefficient. Measure move selection and edit completion
separately; more movement alone is not success.

The [existing-success diagnostic](../reports/2026-09-13-blueprint-success-learning-diagnostic.md)
established that five standardized training successes were replayed 2,596 times,
with positive weights on all labels in at least 2,451 uses. Two exact pose
probabilities increased, three declined, and all remained small. Two late
gradient snapshots showed weak expected rare-example SIL influence. They do not
prove a unique causal failure or justify reciprocal gradient-based coefficients.
The centered policies have no large successes in their own replay buffers.

The earlier suggestion to simply fit these five examples omitted their previous
training exposure. This experiment explicitly treats them as known training
examples and separates their import into centered policies from other changes.

## Fixed contracts and continuity

| Established behavior | Preservation in this slice | Required evidence before the screen |
| --- | --- | --- |
| Complete native-valid original and retained best | Preserve blueprint identities, directed topology, exact endpoints, rates and physical validation | Representative real generated episode, improved commit and failed private draft returning the valid best |
| Fresh products, rates and randomized valid initial layouts | Every new on-policy episode gets a new input slot; share actual slots across arms only | Bound request/initial/slot identities, generation coverage and failures; no fixed fallback |
| Persistent unfinished drafts | Eight streams, 32-action chunks, true 64-action terminal; no reset at an optimizer boundary | One bounded real interruption/restoration across a nonterminal chunk |
| Model and PPO semantics | Width 40, two graph rounds, centered GAE; existing candidate distribution and parameter ownership | Model/input and gradient equivalence at the optimized execution boundary |
| Durable learning | Continue centered weights, both Adam states, actor/critic ownership and RNG state | Child initialization identities and an exact replay within the new runtime |
| Optimized execution | Restore applicable persistent-worker, shared-memory and batched learner capabilities before training | Complete collect/update/evaluate/checkpoint timing, not a reused setting name |
| Bounded diagnostics | Run readiness and attribution as explicit separate jobs; no automatic tail replay or broad audit hook | Actual launch manifest and measured normal schedule |
| Evaluation and artifacts | Independent evaluation RNG; original cohort preserved; raw evidence ignored | No evaluation-to-training references; compact hashes, retained checkpoints and input payloads |

Keep area-first score ordering, area-only reward, fixed external/component
anchors, current pose candidates/rotations, pipe-first routing and existing
routing bounds. No automatic repair/commit, action masks, success-action macros,
reward reshaping, longer episode, critic detachment or model enlargement is a
factor. A failed bounded edit is not an infeasibility result. This slice remains
within the blueprint exception and the
[constructive planner design](2026-09-02.00-constructive-layout-planner.md).

## Five arms, with matched centered initialization

For each learner seed 17/29/41, fork its completed centered update-512 checkpoint
into all five arms. Preserve the model, both optimizers, RNGs and own success
buffer. The parent is at a true episode boundary; reject a parent with a live
episode rather than dropping its draft. Child update zero is an explicit fork,
not fresh learning and not an in-place resume under a changed historical protocol.
Keep child update numbers separate from global learner ordinals: child update
`n` uses global ordinal `512 + n`, including the existing replay RNG derivation.
Restore policy-sampling and PPO-shuffle RNGs exactly; use a separate deterministic
replay-order RNG for balanced draws so they cannot advance either of those streams.

| Arm | Supplied five successes | Replay sampling | SIL coefficient |
| --- | --- | --- | ---: |
| C | None | Uniform over own successes | 0.05 |
| U05 | Same five in every seed | Uniform over all eligible successes | 0.05 |
| B05 | Same five in every seed | Four large and four small sequences per minibatch | 0.05 |
| U50 | Same five in every seed | Uniform over all eligible successes | 0.50 |
| B50 | Same five in every seed | Four large and four small sequences per minibatch | 0.50 |

`U05 - C` measures supplying known successful paths under the existing sampling
and coefficient. B05/U05 and B50/U50 compare composition at fixed coefficients;
U50/U05 and B50/B05 compare coefficients at fixed composition. Report the
interaction `(B50 - U50) - (B05 - U05)` per seed. No comparison imports the old
standardized learner weights or treats the earlier six runs as concurrent controls.
On-policy trajectories and subsequently discovered buffers will diverge. These
are effects of the configured learning regimes, including discovery feedback;
they are not fixed-data, single-gradient causal effects.

Coefficient 0.50 is a predeclared tenfold diagnostic contrast, not a recommended
production value. Do not adapt it to observed gradients or select a value from
the screen's intermediate quality. A null result rules out neither all possible
coefficients nor all ways of composing replay.

### Imported success contract

Import the exact five entries bound by the
[diagnostic provenance](../reports/2026-09-13-blueprint-success-learning-provenance.json):
standardized seed 17 slots 152/1665/1666 and seed 29 slots 509/1179. These were
native policy-generated training successes, never evaluation demonstrations.
Their import is an explicit exception for these diagnostic arms to the original
self-only buffer rule. Call the auxiliary loss mixed-origin imitation, rather
than claiming all its data are the child's self-generated successes.

Keep original source namespaces, chunk/sample references, full segment references,
actual observations/candidates, stage labels and true-episode Monte Carlo returns.
Do not recalculate returns from a shortened three-action path. Keep counters and
time features exactly as recorded in label tensors. Native witnesses authenticate
the positive commits. Validate and cache immutable inputs once at admission.

An imported entry is eligible before child update 1, with `origin=imported` and
`child_admitted_update=0`; its source update number remains separate. Parent own
entries use `origin=parent-own`. Child discoveries use `origin=child-own`, become
eligible only after their actual episode terminates, and retain the next-update
eligibility rule. Keep source occurrence/use counts distinct from child counters.
Deduplicate by the existing physical/action/return identity, preserve all origin
references, and never increment child discovery counts for imported data.

Classify replay strata from actual source input metadata: food/medical are the
large stratum and watered-grass/copper-liquid the small stratum in this registered
generator. Record facility counts to verify that naming remains supported; these
are dataset strata, not general game-size thresholds or pruning rules. A changed
generator or unexpected product requires a new manifest, not guessed membership.

In B arms, sample four independent sequences with replacement from each stratum
and deterministically shuffle their order. New own successes join their matching
stratum. Both strata must exist at initialization; fail instead of silently
substituting. Uniform arms retain eight independent draws with replacement.
All arms retain equal weighting of present stages within a sequence, then the
mean over eight sequences. Do not apply importance correction: changing the
imitation objective's composition is the intended treatment. Keep positive
`R - stop_gradient(V)` weighting, no weight floor, and no SIL critic-head loss.

The current chunk's experience remains on-policy PPO only. Imported labels and
old replay rows must never enter PPO ratios, GAE, critic targets or reward totals.
PPO KL stopping still stops actor and imitation updates together; the existing
critic-head tail preserves actor parameters and actor Adam state.

## Fresh continuation inputs and budget

Each arm executes 128 child updates: 32,768 policy actions and 512 new complete
episodes. Fifteen arms total 491,520 training actions and 7,680 episode executions,
using the same 512 actual new inputs across arms/seeds. This is a paired screen
on one input sequence, not three independent input distributions.

Parent episode counters continue from 256. Use an explicit child-input namespace
mapping the next 512 global slots, 2048--2559, to child slots 0--511. This prevents
reuse of the parent's actual input bank while preserving stream continuity.
Reserve generation seed base 391000000 for this bank and bind the generator's
existing slot/attempt mapping; check for prior use before preparing inputs.
Generate on demand at each first new slot, publish atomically under the existing
input lock, and share the accepted payload with other arms. Do not reject training
inputs because of poor reward, difficulty, familiar geometry or evaluation overlap.

Keep learning rate 0.0003, gamma 1, GAE lambda 0.95, PPO clipping 0.2, four epochs,
64-row minibatches, target KL 0.02, entropy coefficient 0.001 and gradient clipping
0.5. Keep 300,000 ms native training episode allowance. All arms retain the same
sources, native build, catalogs, input draw schedule and resource allocation.

Per-arm total wall ceiling: 3,600 seconds plus 180 seconds for bounded recovery
and finalization; output ceiling: 4 GiB. Matrix process-tree RSS ceiling: 32 GiB;
minimum free disk: 16 GiB. Run one learner arm at a time with its persistent
worker pool, avoiding GPU/worker oversubscription and treatment-dependent allocation.
Maximum matrix wall budget is 18 hours including bounded setup and post-run
diagnostics, excluding implementation. Readiness must forecast a median-cost
screen under eight hours with this schedule; otherwise revise execution design
before launching, without silently enlarging the scientific or resource budgets.
This absolute ceiling does not substitute for the relative throughput gate below.

Use the following fixed arm order to distribute temporal/host drift across the
three seed blocks: seed 17 C/U05/B05/U50/B50; seed 29 B50/U50/B05/U05/C;
seed 41 B05/C/B50/U05/U50. Record host occupancy and phase times. Completion is
action-budget based; an arm that hits a guard is incomplete. Preserve its partial
evidence and report the missing paired comparison. Do not drop a poor arm,
substitute its best checkpoint or extend its budget after seeing performance.

## Evaluation: learning known actions versus transferring useful edits

### Known labels and continuation states

Measure all 15 fixed successful labels at child updates 0/4/.../128: exact-action
log probability and rank, action-kind mass, probability conditional on kind,
value prediction and positive imitation weight. These are training-anchor
measurements and cannot support generalization claims. Share the identical
update-zero result across arms with the same parent seed.

After training, run a separately bounded continuation diagnostic at checkpoints
0 and 128, using eight fixed sampling streams per each of the 15 pre-label states.
Restore the actual state before the useful pose, before its recorded repair, and
before its commit. Reuse the existing strict native reconstruction approach from
`post_pose_recovery.py`; reconstruct each unique state once and retain it. Do not
replace the recorded intervening actions with an assumed three-action trajectory.

Keep the original attempt/revision counters, max-actions 64, native time allowance
and recorded accumulated elapsed time. The native action allowance is therefore
the original remaining horizon, not 64 additional actions. Apply a separate
two-second diagnostic wall guard per branch without changing policy observation
normalization. Preserve physical/input identity, and report actual restored time
features and any guard termination. This differs from the old BC completion helper,
which resets clocks and changes max-actions; do not reuse those overrides.

The policy chooses every action after the restored starting state. No scripted
repair or commit is allowed. Record valid commit, area improvement versus the
starting retained best, attainment of the original demonstrated area, abandonment,
pending edges and conflicts. Separate results by starting stage. Initial best
already satisfying the demonstrated target is an invalid diagnostic start.
Returning a valid unchanged best is safe output, but not successful improvement.

### Transfer and historical continuity

Preserve the historical 32-factory evaluation every four updates, its action
sampling seeds, 64 actions, two-second budget and 8/32/64 milestones. Report its
15-small/17-large strata. Its known request/geometry overlap means it remains a
continuity panel, not the primary evidence of unseen-geometry transfer.

Before training, freeze 16 additional valid large initial factories: eight food
and eight medical, drawn from the existing external generation specification,
with seed base 401000000 and eight admission attempts per case. Select solely by
product, validity and input identity, never by policy score. No request novelty
claim is made: the generator's finite request space repeats requests naturally.
Reject exact or rate-independent geometry duplicates against the old 2,048
training inputs, five anchors, historical panel and earlier accepted new panel
entries during this one-time cohort construction. Retain rejection metadata;
exhaustion fails preparation rather than lowering the identity requirement.

Evaluate this panel at child updates 0/64/128 with two fixed independent sampling
streams per case, shared across arms. Use the same 64-action/two-second budget.
Set panel sampling seeds to `4010000 + 100 * case_index + replicate`, with
zero-based case indices and replicates 0/1. For the anchor continuation probe use
`4020000 + 1000 * source_case_index + 100 * stage_index + replicate`, with stages
pose/repair/commit indexed 0/1/2 and replicates 0--7. Reuse the same sampling seed
at each inspected checkpoint and across arms/learner seeds; do not search for
favorable sampling streams.
After all arms finish, check identity overlap with the actual 512 new training
inputs without resampling those inputs. Primary geometry-transfer estimates use
the common subset absent from every actual training input; also report all 16.
Fewer than 12 eligible cases makes the transfer verdict inconclusive. Keep the
same eligible subset for all seeds, arms and checkpoints; no outcome-based filtering.

Use final child update 128 as the primary endpoint. Historical-panel late-window
secondary results average updates 100/104/.../128. Report own-initial deltas and
paired arm differences, per-factory outcomes, and all three learner-seed results.
Repeated actions, sampling streams and intermediate checkpoints are not independent
training replicates. Three seeds on one input bank are screening evidence, not
a precise population-level significance claim.

### Predeclared interpretation

- **Known-state progress:** an arm's pre-pose autonomous demonstrated-target rate
  exceeds C by at least 25 percentage points in at least two of three learner
  seeds. Report pre-repair/pre-commit results to locate the remaining stage.
  Better label likelihood alone is partial learning, not demonstrated execution.
- **Transfer candidate:** final strict-subset mean area reduction exceeds C in
  at least two seeds, with nonnegative difference in the third; at least two
  distinct eligible factories improve in each of two seeds. This is a practical
  screening gate, not a statistical significance threshold.
- **Small-factory preservation:** pooled historical small-case late-window mean
  may decline from C by no more than the larger of 0.2 percentage points or 10%
  of C's reduction. Show all seed-level differences even if the pooled gate passes.
- **Only imported anchors improve:** evidence for learning supplied examples,
  insufficient for longer normal training; focus next on discovery and transfer.
- **Labels improve but continuation fails:** examine sequence/phase behavior and
  clock/horizon terminations before changing model capacity.
- **No treatment helps:** do not automatically raise the coefficient or run longer.
  Inspect realized positive weights, actor-active epochs and stage-specific effects;
  this finite comparison does not prove an inability to fit or improve factories.

Run all arms to their registered endpoints unless a correctness/resource guard
fails. No online promotion. A later longer-run decision needs transfer and
small-factory gates; passing the known-state gate alone is insufficient.

## Optimized execution is a prerequisite

The blueprint runner currently serializes native steps and uses CPU packed graph
learning. The earlier optimized runner had persistent collectors, shared-memory
delivery, overlapped preparation and batched CUDA learning. The old padded CNN
itself is not this graph model. Inserting that CNN or old curriculum would change
the experiment and restore the wrong contract.

| Established optimization | Required blueprint implementation |
| --- | --- |
| Persistent collectors | Eight native workers for eight persistent streams; checkpoint worker-owned unfinished sessions, stable stream order, explicit worker failure/recovery |
| Shared-memory delivery | Bounded reusable transport for prepared variable-size graph arrays, with lengths/offsets and ownership; no repeated full Python trajectory serialization at each step |
| Overlapped CPU preparation | Prepare the next needed immutable graph/input batch while the learner works; never collect new PPO actions with stale weights or reset an open stream |
| Batched device work | Target CUDA batched learning, preserving disjoint packed graph semantics and stage-balanced SIL; adapt the existing optimized learner supply/device path without introducing a raster/CNN model |
| Parallel evaluation | Use the persistent pool for independent evaluation factories with fixed per-case RNGs and stable output ordering; do not leave the 32-case panel in a serial loop |
| Cached immutable replay/input data | Authenticate once at admission, cache prepared tensors and invalidate on explicit source/schema changes |
| Measured normal operation | Account for generation/admission, collect/prepare/transfer, PPO/SIL update, evaluation, serialization and checkpoint time; no hidden completion audit |

The historical collector study selected eight workers; the later padded-CNN
schedule used sixteen with a different 64-factory workload. Eight streams here
must be backed by eight real workers, not relabeled serial sessions. Validate
this allocation on the blueprint workload instead of copying old timing claims.

Readiness is one explicit bounded job: a fixed small/large generated workload,
one warmup and three measured complete cycles, comparing serial reference with
the restored pool/supply path. Also compare packed graph update execution on
CPU and CUDA with the same model, samples and minibatch sizes. CUDA is the target
learner path. A CPU learner is admissible only if the same-workload measurements
show that it makes the complete parallel schedule faster, while passing the same
semantic/numerical and memory checks. Record that measured decision explicitly;
CUDA initialization failure must never silently fall back to CPU. Device choice
is common to all arms. Reusing the old environment path is not proof of optimized
execution.

### Enforced throughput gate before the first main update

The user's explicit follow-up prohibits repeating the previous effectively
single-core experiment. Enforce this in preparation and launch, not merely in a
report written after training:

- Verify eight live persistent native worker processes, concurrently executing
  work on eight distinct allocated CPU cores. Inspect effective affinity and
  available CPU quota: children must not inherit a one-core parent restriction.
  One Torch thread per worker is allowed to prevent oversubscription; the overall
  workload must be parallel. Give the coordinator/learner a separate CPU allocation.
- Verify the selected learner device and actual batched forward/backward execution,
  including SIL and data transfer. A CUDA model allocation or GPU memory use alone
  does not establish that training executes there. Capture this once in readiness,
  not through a profiler left attached to every training update.
- Measure a complete four-update cycle including its scheduled 32-case evaluation,
  input/setup work, replay preparation, serialization and checkpoint publication.
  Compare one warmup and three measured repetitions against the serial reference,
  with matching actual inputs and action/evaluation budgets. Record cold startup,
  warmed medians, ranges, process-tree memory and device memory separately.
- Require at least **1.5x warmed end-to-end throughput** versus that serial
  reference, in addition to the full-matrix time forecast. This is a minimum
  engineering gate, not a predicted result or proof of optimal hardware use.
  Missing actions, early evaluation truncation, skipped checkpoints or omitted
  required work cannot count as a speedup. If the gate fails, finish execution
  optimization before main training; do not accept the serial path as a fallback.
- Attribute collection, learner, evaluation and storage time explicitly. In
  particular, parallelize evaluation and remove repeated immutable-data work
  when those dominate; a fast learner kernel alone does not pass the gate.
- Bind the launch manifest to the measured worker/device/runtime settings, verify
  them at startup, and reject mismatches with `replay-balance.execution-gate`.
  Keep normal phase counters lightweight. Do not repeat costly readiness,
  optimizer replay or whole-run auditing per arm or at automatic completion.

The [execution primitive slice](../reports/2026-09-13-blueprint-parallel-execution.md)
verifies worker continuity, parallel evaluation and CPU/CUDA update semantics.
The [complete-cycle runtime](../reports/2026-09-13-blueprint-replay-runtime.md)
now measures 2.296x speedup with eight workers and CUDA, with a 5,460-second
conservative forecast. Launch must still authenticate the matching frozen files.

Verify valid physics, candidate identity, tensor/mask/offset isolation, probability
agreement and gradient ownership, including the shared encoder value gradient.
Keep existing numerical tolerances where applicable; report any old/new optimizer
differences rather than claiming bit-exact cross-runtime continuation. Preserve
parameter schema and optimizer parameter ordering. If compatible import cannot
be verified, stop this warm-start design instead of silently reinitializing Adam.

An eight-update readiness fork must exercise both uniform and balanced replay,
nonempty Adam, an imported success, a newly completed own success or a separate
mechanical admission check, KL actor freezing, and interruption/restoration.
Perform one bounded exact update replay within the new runtime. Mechanical
fixture successes never enter the main buffers. Keep readiness policy weights
out of all main arms. End all profiling/replay jobs before the matrix launches.

## Interface and evidence requirements

Implement an explicit versioned `blueprint-replay-balance-experiment-v1` protocol
with parent references, imported-success manifest, replay sampler/coefficient,
training-input namespace, evaluation panels/seeds, runtime identity and limits.
The child fork/restore contract must be explicit; never bypass historical source
binding to load a checkpoint under an unrelated protocol. Existing completed
runs continue to use their frozen runtimes, without internal compatibility shims.

Record per-update discovered versus imported counts, stratum sizes, actual draws,
stage counts, positive/zero weight counts and sums by origin/stratum/stage, SIL
loss, PPO advantage scale, KL and actor-active epochs. Gather these from tensors
already computed for the loss, batch their transfer, and avoid extra forward or
backward passes in the training loop. Optional gradient attribution is a separate
command over selected saved states, never periodic training instrumentation.

Structured failures require stable code, severity, stage, message and entity:
`replay-balance.parent-incompatible`, `source-invalid`, `stratum-empty`,
`input-namespace`, `worker-lost`, `nonfinite`, `resource-exhausted`,
`evaluation-contaminated` and `continuation-state-invalid`. Worker loss or partial
publication cannot advance the durable update or regenerate an unfinished input.

Retain parent references, child checkpoints at 0/64/128, the latest recovery state,
all non-replayable input/experience needed for continuation, imported source chunks,
selection/RNG state and selected native results under ignored artifacts. Preserve
the actual 512 generated inputs and both evaluation panels. Keep raw runs out of
Git; commit implementation, external configuration, concise findings and compact
provenance. Source/binary/data/parent/arm identities and the chosen device, workers,
CPU allocation, measured readiness forecast and evaluation sampling seeds must be
frozen in a launch manifest before any main optimizer update.

## Work order

1. Restore and verify applicable optimized execution for the blueprint contract.
2. Implement the explicit fork/import/sampler/report contracts and bounded readiness.
3. Freeze cohort identities, launch resources and all five arm configurations.
4. Execute the registered screen; then run the separate anchor continuation probe.
5. Report all paired contrasts, failures, preservation gates and remaining uncertainty.

This design records no main learning result. Main training starts only after the
registered readiness and launch checks pass.
