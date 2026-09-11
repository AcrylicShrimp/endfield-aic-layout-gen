# Blueprint PPO with accumulating self-imitation

Status: complete. Both registered runs and all learner, buffer and native audits pass.

## Observed outcome

Success retention and positive-weight replay work, but autonomous quality improves
only slightly in this screen. Final mean validated area reduction is **0.83% for
PPO and 1.67% with self-imitation**, a difference of .83 percentage points. Both
are below their matching untrained mean of 7.92%. This does not establish a robust
learning benefit or solve the observed deterioration during training.

Each row below covers six two-second cases on the same factory: three original
starts and three overlap starts. Initial retained-best area is 700 in every case.

| Seed | Initial, both arms | Final PPO | Final PPO + self-imitation |
|---|---:|---:|---:|
| 17 | 9.17% | 1.67% | 2.50% |
| 29 | 6.67% | 0.00% | 0.83% |
| Mean | 7.92% | 0.83% | 1.67% |

Final treatment has two area wins, one loss and nine ties among 12 paired cases.
With the complete area/physical-tile/turn score, it has five wins, three losses
and four ties; an area tie can still lose on transport geometry.
One control case reaches area 630; treatment reaches 630 in one case and 665 in
two. The remaining cases retain area 700. At the registered action milestones
8/32/64/128/256, **both final policies have zero mean area improvement** and all
12 cases reach every milestone. Their few area gains occur later in the timed
run. The two-second results should therefore not be read as rapid compaction.

Treatment seed 17 moves facilities much more frequently: 10.94% of final actions,
versus .54% for control. Seed 29 is .86% versus .18%. More movement and a retained
success buffer do not by themselves demonstrate reliable sequence completion.
The intermediate curves also fluctuate: control seed 29 reaches 11.67% at update
64 and then falls to zero; treatment seed 29 reaches 3.33% and falls to .83%.
Two seeds and repeated samples on one fixture are a small descriptive comparison,
not independent generalization trials.

## Discovery, admission and reuse

Control archives 12 positive-area commits and retains 11 unique sequences.
Treatment archives 25 and retains 22. Every one is discovered by the current
training policy, with no supplied area improvement. Duplicates remain occurrence
records. An episode can have several improvements, so commit counts are not
counts of independently successful episodes: 11 control and 24 treatment episodes
improve area, out of 256 episodes per arm.

| Arm / seed | First positive commit update | First admission | First replay draw | First positive-weight replay | Unique sequences |
|---|---:|---:|---:|---:|---:|
| PPO / 17 | 82 | 88 | — | — | 1 |
| PPO / 29 | 1 | 8 | — | — | 10 |
| Self-imitation / 17 | 54 | 56 | 57 | 57 | 5 |
| Self-imitation / 29 | 1 | 8 | 9 | 16 | 17 |

The earlier first discovery in treatment seed 17 cannot be credited to replay:
that seed had no admitted experience and no imitation gradient before the first
success. Both arms have identical initial model, both Adam, sampler, shuffle,
native stream and empty-buffer states. Subsequent trajectories include actual
wall-clock observations, so matching seeds do not make them identical.

Treatment makes 80,090 label draws, of which 71,370 (89.11%) have positive weight.
Seed 17 has 31,546 positive draws out of 31,546 and positive imitation in all 72
updates with an eligible buffer. Seed 29 has 39,824 out of 48,544 (82.04%); 99
updates have positive imitation and 21 draw experience whose weights are all zero.
This confirms the user's current-value filter is active. It does not support
attributing the overall weak result to complete gating: most drawn labels do
receive positive weight.

The 22 treatment entries retain 79 label references, with three to seven labels
per sequence. The first entries are drawn 4,082 and 4,418 times in seeds 17 and
29. Their source labels remain in use through the final update, at ages up to
74 and 127 optimizer updates. Total sequence draws are 24,352. There is no
eviction or discarded archived success in either completed arm.

These are repeated label occurrences, not 80,090 unique examples. A positive
weight establishes a contribution to the auxiliary loss; it does not prove that
the policy reliably reproduces the full successful sequence during autonomous
inference. The observed movement increase and weak completion outcomes should
be considered together.

## Fixed experiment

The user approved retaining rare policy-discovered successes, then supplied the
positive-return-advantage SIL formulation. The
[registration](../designs/2026-09-11-blueprint-success-replay.md) incorporates that
weighting before the main run. This screen adds a positive-advantage weighted
actor imitation objective to PPO. It uses uniform sequence sampling and the
existing PPO critic loss; it does not implement prioritized replay or a separate
SIL value objective.

Each arm starts fresh seeds 17 and 29 and an empty per-seed success buffer. There
are 128 updates per seed, eight persistent streams, 32 actions per chunk and 256
actions per actual episode: 65,536 on-policy actions per arm. Original and overlap
starts are equally represented. No external demonstration, previous-run success,
repair-ready start, supplied compaction, evaluation transition or scripted action
enters main training. The coefficient is 0 for control and .05 for treatment;
all other configuration, model and native runtime identities match.

The graph actor/critic has width 40, learning rate 3e-4, gamma 1, GAE lambda .95,
four PPO epochs, minibatches of 64 and target KL .02. Each actor-active minibatch
draws eight unique-population sequences uniformly with replacement. Imitation
stops when KL stops actor updates; the existing four-epoch critic-head tail stays.

Evaluation occurs at updates 0/32/64/128, with three sampling streams for each
original/overlap start and two seconds of native acceptance time. Fixed policy
action milestones are 8/32/64/128/256. These two starts differ from the earlier BC
evaluation curriculum, so previous absolute evaluation means are not controls.
Both runs retain the 1800-second and 3-GiB per-arm guards.

## What is retained and replayed

Every native-validated positive-area commit is archived, including a routing-only
success if discovered. Its edit segment begins after the most recent accepted
restore or successful commit, or at episode start. Full segment references remain
available; imitation selects physically changed accepted poses, changed repairs
and the final commit. Selection is not a causal proof that every label was needed.

Archive discovery and learner admission are different times. Admission waits for
the real episode terminal so each label receives its actual remaining-episode
discounted return, including later rewards. A short segment return would not match
the PPO critic's horizon. For each replay prediction, the label weight is
`max(R - stop_gradient(V_current), 0)`, in raw native reward units. Stage means
balance movement, repair and commit; sequence means balance length. The .05
coefficient is applied afterward, without weight floors or normalization.

Unique physical-state/action/return sequences stay in the bounded buffer. A
duplicate increments its occurrence count but does not change its uniform draw
probability. Clock and counter features are excluded only from deduplication;
the policy still learns from the actual original inputs. Every replay source
comes from an earlier optimizer chunk and an already completed episode.

The new checkpoint envelope saves both Adam states, model, RNGs, native stream
snapshots, unique sequences, duplicate/draw counts and unfinished edit segments.
Immutable sample references avoid duplicating observations in every checkpoint.
The empty-buffer and zero-coefficient paths call the existing PPO update directly.
Replay labels do not enter PPO ratios, GAE, native rewards or critic targets.

## Verification and scope

All 512 main learner updates reconstruct exactly, including both Adam states,
sampling and shuffle RNGs, credit targets and buffer transitions. Every main
training action is reselected from its original distribution and sampling RNG;
the mechanical-readiness script is not used. Archived successes, sampled valid
commits and episode/evaluation final bests undergo independent native admission.
Selected evaluation log probabilities and values are recomputed from frozen
checkpoints. All buffer archives, admissions, duplicates, uses and retained
checkpoint open segments agree with the replayed states.

| Audited evidence | PPO | PPO + self-imitation |
|---|---:|---:|
| On-policy training actions | 65,536 | 65,536 |
| Exact learner/buffer update replays | 256 | 256 |
| Native stream snapshot restorations | 2,048 | 2,048 |
| Continuing boundaries / nonterminal bootstraps | 1,792 | 1,792 |
| Completed training episodes | 256 | 256 |
| Timed evaluation cases | 48 | 48 |
| Charged evaluation actions | 84,942 | 85,157 |
| Commit/final witness references | 28,502 | 27,701 |
| Unique native-admitted witnesses within arm | 384 | 413 |

Control and treatment take 534.08 and 538.74 seconds. Mean learner-update time is
.245 and .293 seconds; total wall time also includes collection, checkpoint I/O,
buffer resolution and evaluation. Actual actor epoch totals are 997 and 1,014,
while every update retains four critic epochs. On-policy action budgets match;
auxiliary compute and actual actor step counts are not fixed to be equal.

All 96 evaluation cases stop through the elapsed guard. The maximum reported
elapsed times are 2546.19 ms for control and 2366.83 ms for treatment. The budget
is native acceptance time, not a strict process-return deadline. All 16 / 8 cases
exceeding the budget by more than 10 ms are checked to end with an uncharged
rejection that preserves the draft revision and retained-best score.

Before main execution, native tests cover multiple improvements in one episode,
true-terminal returns across chunk boundaries, duplicate retention, restore and
episode boundaries, rejected/no-op filtering, routing-only success admission,
buffer guards and source-age checks. Learner tests cover empty/zero parity,
positive/nonpositive weighting, gradient ownership, KL freeze and exact replay
from nonempty Adam and buffer state. Stochastic readiness, explicit mechanical
readiness and actual CLI resume on a copied nonempty checkpoint pass. Mechanical
readiness is a declared fixture and is never represented as on-policy discovery.

The Python suite runs 389 tests with 7 skipped. Workspace check passes; Rust tests
have 563 passes and 5 ignored. No Rust code or dependencies change in this slice.

This experiment preserves the approved blueprint-edit contract: private drafts
can contain overlap and pending original connections, but only complete physical
validation can replace the retained best. Directed topology, endpoints, rates and
identities remain frozen. Review against the constructive-planner design and
AGENTS policy finds no constructor, production fallback or exact-solver cutover.

## Reproduction and retention

The [compact provenance manifest](2026-09-11-blueprint-success-replay-provenance.json)
binds selected evidence and source identities. Figures and the complete machine
summary are under `artifacts/factory-edit-pilot/blueprint-success-comparison-20260911/`:
`comparison.png`, `buffer.png`, `policy-improvement.png` and `summary.json`.
The improvement figure shows the first treatment training success, seed 17,
update 54, overlap-start episode action 182: validated score `[630, 125, 16]`.
Its ten-action recorded segment supplies three imitation labels. It is a training
discovery example, not the final policy's best evaluation result.

Use fresh output paths for repeated training. The release native binary and all
runtime sources are hash-bound by each run's protocol. Generated outputs and
non-replayable experience remain under ignored `artifacts/factory-edit-pilot/`.
Selected optimizer/native checkpoints include initialization, evaluation, the
first buffer admission and the following update. Other optimizer checkpoints
are reconstructible from retained chunks and have been removed by the runner.
A seed alone does not reproduce wall-clock observations or future trajectories.

```sh
P=tools/factory-edit-pilot/.venv/bin/python
$P tools/factory-edit-pilot/blueprint_success.py train --config tools/factory-edit-pilot/blueprint-success-replay.json --native-library target/release/libaic_pilot_native.dylib --output artifacts/factory-edit-pilot/blueprint-success-control-20260911 --coefficient 0
$P tools/factory-edit-pilot/blueprint_success.py train --config tools/factory-edit-pilot/blueprint-success-replay.json --native-library target/release/libaic_pilot_native.dylib --output artifacts/factory-edit-pilot/blueprint-success-treatment-20260911 --coefficient 0.05
$P tools/factory-edit-pilot/audit_blueprint_success.py --run artifacts/factory-edit-pilot/blueprint-success-control-20260911
$P tools/factory-edit-pilot/audit_blueprint_success.py --run artifacts/factory-edit-pilot/blueprint-success-treatment-20260911
$P tools/factory-edit-pilot/summarize_blueprint_success.py --control artifacts/factory-edit-pilot/blueprint-success-control-20260911 --treatment artifacts/factory-edit-pilot/blueprint-success-treatment-20260911 --output artifacts/factory-edit-pilot/blueprint-success-comparison-20260911
```

`blueprint_success.py resume --output RUN` resumes the last durable checkpoint.
For readiness, supply `--readiness` or `--mechanical-readiness` to `train` with a
new output path. The latter is explicitly scripted and checks buffer mechanics.
