# Fresh random blueprint inputs: advantage scaling screen

Status: complete. Six registered arms finished; final analysis and tracking readback passed.
Completed on 2026-09-13 KST (2026-09-12 UTC).

## Question and ordering

After restoring the previously required fresh random products, rates and initial
geometry in `6e7a3e5`, compare standardized GAE with centered-only GAE. The
restoration is documented in
[its report](2026-09-12-blueprint-random-generation-restoration.md). The preceding
capacity screen trained on a fixed contract fixture and is not evidence about
this restored training distribution.

The comparison was registered in `f313821`, after restoration readiness and
before main training. Both conditions use newly initialized width-40 models with
two message rounds, learner seeds 17/29/41, empty Adam states and empty success
buffers. The actor consumes either `(GAE - mean) / max(std, 1e-8)` or
`GAE - mean`. Critic target construction and all other PPO settings remain unchanged:
learning rate .0003, gamma 1, lambda .95, clip .2, four epochs, minibatches of 64,
target KL .02, entropy coefficient .001, gradient clipping .5 and success replay
coefficient .05. Centering changes the scale of PPO relative to entropy and
success replay as well as suppressing inverse-standard-deviation amplification;
this experiment does not separate those mechanisms. Equal recorded rollouts
produce identical value targets in both modes; the trained policies collect
different rollouts, so their observed target values need not match.

## Registered comparison

- Six arms, 512 updates each: 131,072 policy actions and 2,048 complete episodes
  per arm; 786,432 training actions and 12,288 episode executions total.
- Eight streams, 32-action chunks, 64-action episodes. The same 2,048 actual
  generated input records are shared across arms in identical order. No input
  slot is recycled within a learner. The three learner seeds share one training
  input sequence rather than constituting three independent data draws.
- Input seed base 361,000,000, up to eight distinct admission attempts per slot;
  the original four-product generation specification is unchanged.
- Evaluate at updates 0, 4, ..., 512 on the authentic 32-factory historical
  cohort. Final and last-window mean (updates 388 through 512, 32 evaluations)
  are the predeclared primary endpoints. No peak checkpoint is selected.
- Four pinned, single-thread CPU workers at most, on logical CPUs 0, 1, 4 and 5.
  Each learner has a three-hour active-time ceiling plus 180 seconds to finish
  an open episode,
  an eight-GiB output ceiling, and an eight-GiB filesystem reserve. Resource
  termination before update 512 is incomplete rather than a matched outcome.

The main specification SHA-256 is
`0b864fa92781abc4b1498a14778fe74958495f069817d1650f8b78927079a89e`.
The model, native library, generator, catalogs, input payloads, checkpoints,
updates, evaluations and terminal audit are bound by the retained protocols.

## Readiness evidence

Before main training, a separate six-arm, eight-update matrix used seed base
371,000,000. All arms completed: 12,288 training actions, 192 episode executions
over the same 32 actual inputs, and 576 evaluation cases, all completing 64
policy actions. The accepted inputs covered 11 copper-liquid, four food, eight
watered-grass and nine medical requests, with 26 distinct requests and 31
distinct geometries. There were 34 admission attempts for 32 accepted inputs;
both rejected attempts remain recorded. Shared input identity was verified as
`d339511a9b11c232a8c6a7976b13c4e05c890b1e6491e03023cdf0d396993aca`.

Both seed-17 readiness arms also passed full-run audits: eight exact optimizer
replays, 64 native restorations, 32 stream-continuity checks, 32 completed
training episodes and 96 evaluation cases each. Standardization had 1,536
positive self-imitation labels; centering had 3,840. This exercises nonempty
success buffers and Adam states in both branches. The audits independently
validated 160 and 157 unique witnesses respectively. Full audits were preserved
separately, and the original tail audits were restored and their completion
bindings checked afterward.

All six readiness tracking runs finished. The comparison CLI verified paired
initial weights, all non-factor configuration/source/runtime fields, identical
actual input order, evaluation cadence and immutable common policy probes.
Readiness did not change the registered main budget or choose a winner.

## Results

Centered-only credit has higher final and late-window means in all three paired
learner seeds. The absolute quality gain is weak and uneven: every final
32-case median is zero. Centering wins the registered 32-case comparison at this budget. Its advantage
is not robust to excluding overlapping training geometries, so this does not
establish a generally better policy or broadly effective learning.

All values below are percentages of original validated factory area saved;
own-initial changes are percentage points. Late means cover the registered
32 evaluations at updates 388--512.

| Credit | Seed | Initial | Final | Own-initial change | Late mean | Final improved cases |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Standardize | 17 | 0.81656 | 1.98004 | +1.16348 | 1.37396 | 9/32 |
| Center | 17 | 0.81656 | 2.53769 | +1.72112 | 2.00601 | 7/32 |
| Standardize | 29 | 0.66031 | 0.00000 | -0.66031 | 0.83569 | 0/32 |
| Center | 29 | 0.66031 | 0.94952 | +0.28921 | 1.01402 | 2/32 |
| Standardize | 41 | 0.97281 | 0.84374 | -0.12907 | 1.24662 | 5/32 |
| Center | 41 | 0.97281 | 2.65282 | +1.68000 | 1.64765 | 7/32 |

The three-seed final means are **0.94126% standardize versus 2.04667% center**
(+1.10541 percentage points). Late means are **1.15209% versus 1.55589%**
(+0.40380 points). Final own-initial changes average +0.12470 and +1.23011 points.
These three-seed means are descriptive, without a population-significance claim.
Final matched per-factory area comparisons give center 12 wins, 79 ties
and five losses over 96 seed/case pairs. These compare the two policies on each
factory, whereas improved-case counts compare each policy with the input factory.

For nonzero final results, the top three factories contribute 58.97% and 65.53%
of standardization's summed area reductions (seeds 17/41), and 67.99%, 100% and
62.98% for centering (17/29/41). Standardized seed 29 improves no factory, so
its concentration is undefined. The higher centered average is not equivalent
to more improved factories in every seed: seed 17 improves seven rather than nine.

### Input-overlap sensitivity

These post-hoc subsets reuse the recorded evaluations and average the same
three seeds. They do not replace the registered primary population.

| Evaluation subset | Initial, both | Standardize final | Center final | Standardize late | Center late |
| --- | ---: | ---: | ---: | ---: | ---: |
| Registered 32 | 0.81656 | 0.94126 | 2.04667 | 1.15209 | 1.55589 |
| Exclude two exact initial files: 30 | 0.87100 | 0.83307 | 1.84124 | 1.05261 | 1.43526 |
| Exclude seven matching geometries: 25 | 0.91187 | 0.66122 | 0.98897 | 0.96073 | 0.79827 |

The 25-case late-window ranking reverses, and center is below its untrained
subset baseline there. Its final point remains slightly above baseline
(+0.07711 points), while standardization is -0.25064 points. Centered seed 29's
two final improvements both occur in excluded geometries: its 25-case final is
zero. Thus the overall centered advantage depends materially on the evaluation
composition; consistent superiority on nonoverlapping initial geometry is not
established. The subsets still contain seen requests and product families.

### Learning over the run

There is a positive learning trend despite low absolute quality. Averaging the
32 evaluations within each successive 128-update block and then the three
learner seeds gives:

| Update block | Standardize | Center |
| --- | ---: | ---: |
| 1--128 (evaluations 4--128) | 0.44917 | 0.62700 |
| 129--256 | 0.63970 | 0.93941 |
| 257--384 | 0.88189 | 1.40207 |
| 385--512 | 1.15209 | 1.55589 |

Both begin below the untrained mean of 0.81656%, then recover and exceed it.
Every individual arm's final block exceeds its first block. The block sequence
is not monotonic in every arm: centered seed 29 dips in block two and centered
seed 41 dips in the final block. Single evaluations fluctuate substantially.
These are additional descriptive block summaries from the existing records,
not extra registered endpoints or proof that a longer run will keep improving.
Low final medians and concentrated gains do not mean there is no learning;
the aggregate trend and limited breadth of improvement are separate findings.

The scoped tracking view is
[Fresh random factories: advantage standardization vs centering](https://wandb.ai/acrylicshrimp-personal/endfield-aic-layout-gen?nw=ooq0svqw5q6).
All six tracking runs finished. The final view readback matches all 774 durable
evaluation points, including update/action axes, area reduction and own-initial
delta. No recorded evaluation was cut short: 24,768 cases all performed 64
actions, totaling 1,585,152 evaluation actions.

The registered final tail audits passed: 12 exact optimizer replays, 96 native
restorations, 48 continuity checks, 48 completed training episodes and 192
final evaluation cases (12,288 actions) across the six arms. They validated
498 distinct witnesses when counted within each arm and summed; this is not a
claim of 498 globally unique witnesses. No extra full-run main audit was added.

## Credit, critic and action diagnostics

| Credit | Seed | Zero-reward batches / 512 | Positive commits | Retained successes | Final fixed-input pose probability | Final sampled-return MAE |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Standardize | 17 | 318 | 250 | 250 | 12.67% | 0.01402 |
| Center | 17 | 308 | 287 | 287 | 69.93% | 0.00825 |
| Standardize | 29 | 425 | 104 | 104 | 21.94% | 0.00327 |
| Center | 29 | 355 | 206 | 206 | 84.52% | 0.00545 |
| Standardize | 41 | 322 | 240 | 238 | 9.24% | 0.00920 |
| Center | 41 | 318 | 311 | 311 | 69.16% | 0.02137 |

All modes retain SIL-style successful-trajectory replay at coefficient .05,
weighted by positive return minus the current value estimate. It is active in
both conditions. Its coefficient alone does not describe its influence relative
to the PPO term: centering changes that term's scale. This was an advantage-mode
comparison with SIL enabled, not PPO without SIL or an SIL-weight experiment.

Untrained fixed-input pose probabilities were 21.88%, 20.29% and 20.51% by seed
in both modes. Their final three-seed means are 14.62% standardized and 74.54%
centered. On actual final evaluation trajectories, pose actions comprise
10.55/14.55/8.50% under standardization versus 66.21/69.48/67.92% under centering.
Standardization spends 33.64--42.92% of actions committing and 29.15--40.63%
repairing all connections; centering spends 8.30--12.84% committing and
5.03--13.48% repairing all. Movement frequency explains policy behavior but
is not itself evidence of valid improvement.

Standardization's inverse-STD multiplier has all-update medians 291.78, 799.73
and 300.61. On zero-reward batches alone, those medians are 491.52, 972.43 and
582.50; maxima are 4,578, 33,016 and 7,245. Centering's multiplier is one.
All-update raw advantage STD medians are .00343/.00125/.00333 standardized and
.00646/.00511/.00509 centered. The observed amplification is real, but a
zero-reward batch's GAE can contain legitimate bootstrap/value differences as
well as estimation error. These records do not isolate pure critic noise or
prove it is the cause of the score difference.

Last-window training GAE-target MAE averages .00394 standardized and .00547
centered. Final sampled evaluation return errors are mixed by seed, including
higher centered error in seeds 29 and 41. The policies visit different states
and generate different target distributions; lower raw critic MAE is not an
apples-to-apples accuracy or policy-quality ranking. Last-window training area
reduction is .92855% standardized and 1.57439% centered over completed episodes.

## Actual generated input coverage

The final analyzer verified all six arms actually consumed the same ordered
2,048 inputs and completed all episodes. The ordered input digest is
`2611944b6ec176f9fa7213067228119ceef4ead266882ec811da465af9554373`.
It also verified fresh paired weights, empty initial optimizers/success buffers,
non-factor settings, sources/native/runtime, evaluation cadence, completion
bindings and all 774 checkpoint identities used for the common immutable probes.
No training or trajectory replay was added by this final analysis.

The shared store published all 2,048 inputs: 533 watered-grass, 518 copper-liquid,
514 medical and 483 food. They comprise 194 distinct requests and 1,723 distinct
geometries. All 39 rejected admission attempts report `construction-stopped`;
their evidence is retained. The 2,048 accepted setups took 422.46 seconds in
total, with 1,993 changing geometry relative to the constructor output. Identical
draws or unchanged geometry are reported rather than filtered out.

The historical cohort is independently sourced, but it is not strictly disjoint
from this finite random input distribution. All 32 evaluation request files
occur in the generated store; two evaluation initial-layout files also match
exactly (generation IDs 232700016 and 232700018). No evaluation input was used to
select or generate training inputs. Independent seeds alone do not guarantee
unseen requests or geometry. Keep all 32 registered cases as the primary result,
and report the 30 cases excluding exact initial-file overlaps as a secondary
sensitivity check from existing reports. Using the generator's existing
`geometry_identity` (physical geometry excluding rates/utilization) also finds
seven overlapping initial geometries: IDs 232700005, 232700012, 232700013,
232700016, 232700017, 232700018 and 232700025. The remaining 25 form a second,
stricter geometry sensitivity subset. Both filters are based on input identity,
not outcomes, and were identified before the final learner seed completed.
Neither changes the registered 32-case primary evaluation. Request/product
generalization beyond the training distribution remains unmeasured.

After publication, 4,135 regenerable constructor HTML files and accepted
candidate reports were removed, freeing 7,507,136,119 bytes. Their hashes and
paths are recorded in the retained cleanup plan. Actual episode payloads,
constructor and randomized inputs, setup traces, generation/failure records and
all learner/evaluation evidence remain. The accepted constructor geometry was
matched to its preserved admitted input before removing candidate histories.

## Execution cost and the earlier optimization

The user asked why the experiment remained slow despite previous optimization.
The [earlier native collector study](2026-09-10.12-native-worker-scaling.md)
selected eight persistent collector workers. That worker pool is absent from
the new blueprint learner. Its eight streams are native sessions in one process:
policy inference is packed across streams, but native steps execute serially.
The historical evaluation cohort also runs one factory after another. Each
experimental arm is pinned to one CPU; four arms can run concurrently, leaving
two initially queued. Eight streams must not be described as eight workers.

The new contract has packed graph execution and success-input caching, but it
does not inherit all execution optimizations from the previous learner. This
performance gap should have been checked when introducing the new training path.
The registered experiment has 786,432 training actions and 1,585,152 evaluation
actions. Frequent evaluation is substantial work in its own right.

Final summed phase counters attribute 21.16% of active time to collection,
15.76% to optimizer updates, 30.63% to evaluation execution and .68% to
evaluation admission. The remaining 31.77% was not separately instrumented;
serialization, hashes, input/buffer handling and other harness work occur there.
These are elapsed phase counters, not a sampled CPU profile. Collection already
includes input access, so that cost must not be added again. Evaluation's
`inference_seconds` includes native environment and graph preparation work as
well as neural inference. Neither counter isolates matrix multiplication time.

The main experiment keeps its registered execution settings. Missing established
execution capabilities are unfinished migration obligations, not newly proposed
optional features. Their restoration needs measurement and semantic checks on
the blueprint contract; the old worker count cannot simply be copied and assumed
equivalent for a different model and workload. The general continuity rule is
now recorded in `AGENTS.md` by `41a9707`. Its scope includes all established
requirements, decisions, solved problems and operational knowledge, not only
performance.

The existing frozen job also performs an automatic tail audit before releasing
its CPU slot to the next arm. This extends matrix wall time beyond the learner's
active-time counter; the learner's duration/grace guard does not bound that
automatic post-training audit. The user subsequently required expensive verification to
be necessary, bounded, explicitly one-off and isolated from normal long runs;
`a05fa9f` records that policy. No additional full-run main audit is requested for
this comparison. The current frozen matrix's previously registered terminal
gates remain recorded, and must not be mistaken for a suitable default normal
execution design.

The completed matrix took 1h 50m 51.778s wall time and 5h 21m 13.675s CPU time
according to its systemd journal. Its 43.7-GiB cgroup memory peak includes page
cache and is not process RSS. Per-arm recorded outputs were 5.41--7.08 GiB,
below the eight-GiB guard; approximately 13.08 GiB filesystem space remained.
The training service and tracking observers finished; temporary tracking
credentials were removed from the server experiment directory.

| Credit | Seed | Active minutes | Post-learning completion minutes |
| --- | ---: | ---: | ---: |
| Standardize | 17 | 56.99 | 7.88 |
| Center | 17 | 47.08 | 7.17 |
| Standardize | 29 | 51.10 | 3.22 |
| Center | 29 | 47.00 | 5.27 |
| Standardize | 41 | 51.67 | 6.56 |
| Center | 41 | 38.97 | 7.10 |

Post-learning time is `completion.finished_at - state.finished_at`; it includes
tracking/finalization and the automatic audit, rather than isolating audit CPU
time. Parent wall time also includes queueing. Earlier arms competed with four
workers; the last seed pair ran with two. Action budgets and source contracts
are matched, but elapsed input features prevent treating worker timing as
semantically irrelevant.

## Measurement boundaries

Area reduction is measured from each evaluation factory's original valid
footprint, using the retained completely validated best. Own-initial deltas
compare with that learner's actual untrained evaluation. The probe movement
probabilities use the exact same 32 immutable initial observations from the
standardization seed-17 untrained evaluation for every model and checkpoint.
Movement alone is not a quality objective.

The critic's evaluation return error compares predictions with sampled remaining
undiscounted reward in the recorded final evaluation traces, not an exact value
function. Training target MAE compares with recorded GAE targets. Elapsed time
remains a policy feature, and the two-second evaluation guard remains part of
the protocol. Actual guard coverage and CPU assignments must accompany results.
The study is a finite action-budget screen; it does not establish convergence,
long-run superiority, or performance beyond the four training product families
and the existing 32-factory evaluation cohort.

## Reproduction and retained evidence

Use frozen source `f313821` and the preserved Linux CPython-3.12 native runtime.
The run used PyTorch 2.14.0+cu130 on CPU. Build with the selected interpreter:

```sh
PYO3_PYTHON="$PILOT_PYTHON" cargo build --release -p aic-cli -p aic-pilot-native \
  --features aic-pilot-native/extension-module
"$PILOT_PYTHON" tools/factory-edit-pilot/blueprint_advantage.py prepare \
  --spec tools/factory-edit-pilot/blueprint-advantage-screen.json \
  --native-library target/release/libaic_pilot_native.so \
  --evaluation-cohort "$AUTHENTIC_COHORT" --env-file "$TRACKING_ENV" \
  --output artifacts/factory-edit-pilot/advantage-main
"$PILOT_PYTHON" tools/factory-edit-pilot/blueprint_advantage.py run \
  --output artifacts/factory-edit-pilot/advantage-main \
  --env-file "$TRACKING_ENV" --cpu-slots 0 1 4 5
"$PILOT_PYTHON" tools/factory-edit-pilot/compare_blueprint_advantage.py \
  --matrix artifacts/factory-edit-pilot/advantage-main \
  --output artifacts/factory-edit-pilot/advantage-main-analysis.json
```

These commands make a new execution; they cannot recreate non-replayable elapsed
features from seeds alone. Preserve the actual generated inputs, learner chunks,
checkpoints, evaluation cases and source-bound native runtime to audit this run.

The retained server root is
`/home/ashrimp/Srv/aic-blueprint-advantage-20260912`; outputs are under its ignored
`artifacts/factory-edit-pilot/`. The local evidence root is
`artifacts/factory-edit-pilot/blueprint-advantage-20260912`. Raw run outputs are
excluded from Git. The local mirror contains JSON metadata, analysis and the
retained native runtime; actual input payloads and compressed learner/evaluation
checkpoints remain on the server. This is not a complete local backup. Before the run, prior capacity traces were relocated with
complete file/hash verification, as recorded in
[the relocation manifest](2026-09-12-blueprint-capacity-artifact-relocation.json).

The compact [provenance manifest](2026-09-12-blueprint-fresh-input-advantage-provenance.json)
binds the selected analysis, secondary arithmetic, quarter trends, input coverage,
cleanup plan, tracking readback, service journal, runtime and verification logs.
The plotted artifact is `advantage-curves.png` under the local evidence root;
it displays all six complete curves and was visually inspected.

Secondary arithmetic reads only the existing evaluation/update JSON. Subset
means filter by the documented generation IDs; the late window is `update > 384`.
Quarter means use `128*i < update <= 128*(i+1)`. Concentration divides the top
three reductions by their total, and is undefined when total improvement is
zero. Timing sums recorded phase counters without double-counting input access
or inference inside collection/evaluation. The small executed arithmetic script
and output are retained locally with hashes. This stage makes no new rollouts,
exact replays or benchmark runs. Required artifact/index checks accompany the
final documentation commit; successful source verification was not repeated.

Verification before the main experiment: Python 424 tests (417 passed, seven
skipped); Rust workspace check and tests passed (564 passed, five ignored).
Real native generation, continuation and exact self-imitation replay tests were
enabled. Artifact and staged-file policy checks passed for registration.
