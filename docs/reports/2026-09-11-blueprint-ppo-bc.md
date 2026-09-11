# Blueprint PPO with a small behavior-cloning auxiliary loss

Status: complete. On this fixture, PPO plus .05 BC improved final mean validated
area reduction from **2.08% to 10.00%**. Both treatment seeds compacted every
original/overlap evaluation case from 700 to 595. The treatment also improved
held-out target completion, but same-pose completion did not increase. All 256
learner updates replay exactly and all retained results pass native validation.

## Observed outcomes

Each final mean below covers six two-second cases: two original, two overlap,
and two already-improved starts. Gains are relative to the post-prefix retained
best, so the already-improved starts contribute zero for staying at 595.

| Model seed | PPO | PPO + BC (.05) |
|---|---:|---:|
| 17 | 0.00% | 10.00% |
| 29 | 4.17% | 10.00% |
| Mean | 2.08% | 10.00% |

All eight treatment original/overlap cases reach area 595, a 15% reduction from
700. Control reaches 630 in one original case and 595 in one overlap case; the
other six stay at 700. Among 12 matched final cases, treatment has seven area
wins, five ties and no losses. With the complete lexicographic score, it has ten
wins and two losses: ties in area can still regress in physical tiles or turns.
These repeated cases on one fixture are descriptive, not independent trials.

The advantage also appears at matched action counts; all 12 cases in each arm
reach every milestone below. These are the final policies' aggregate gains.

| Policy actions | PPO | PPO + BC (.05) |
|---|---:|---:|
| 8 | 0.00% | 0.83% |
| 32 | 0.00% | 4.58% |
| 64 | 0.00% | 5.00% |
| 128 | 0.00% | 5.83% |
| 256 | 0.00% | 7.92% |

Final movement share is .05% / 3.21% for control seeds 17 / 29, versus 5.12% /
5.48% with BC. Training produces 11 versus 80 positive-area commits. Excluding
repair-ready starts gives two versus 52 original/overlap positive-area commits,
and two versus 33 improved original/overlap episodes. Counts of commits and
counts of episodes are different because an episode can improve repeatedly.
The first treatment original-start training improvement occurs at seed 17,
update 18, action 45, with score `[630, 129, 22]`.

All four treatment overlap evaluation cases first clear overlap using
`restore_best`, at actions 4/8 for seed 17 and 2/1 for seed 29, before compacting.
This does not demonstrate new multi-move overlap-resolution ability.

## Held-out sequence behavior

Four examples from three held-out physical groups, two sampling streams and two
model seeds give 16 cases per starting position at each checkpoint. The target
is the teacher's validated area or better, within 64 autonomous policy actions.

| Final policy outcome | PPO | PPO + BC (.05) |
|---|---:|---:|
| Target reached from before the move | 0/16 | 6/16 |
| Target reached from after the move | 3/16 | 7/16 |
| Commit before any accepted move/restore from the post-pose draft | 3/16 | 3/16 |

The last row also reaches the corresponding target area in all six observed
commits across both arms. Thus the improvement cannot be attributed solely to
finishing the existing post-pose draft. Additional moves and/or restoration
contribute to the treatment's eventual outcomes. Before training, both arms
reach 0/16 pre-pose and 2/16 post-pose targets under these action-bounded probes.

Teacher-action likelihood helps distinguish the learned behavior. The following
are geometric mean probabilities of the *specific demonstrated action* over the
four held-out examples; the pose column is not total movement probability.

| Seed / final policy | Demonstrated pose | Repair all | Commit |
|---|---:|---:|---:|
| 17 / PPO | 0.0008% | 25.08% | 27.77% |
| 17 / PPO + BC | 5.61% | 34.57% | 21.08% |
| 29 / PPO | 0.0564% | 25.88% | 28.14% |
| 29 / PPO + BC | 2.51% | 38.71% | 32.26% |

Untrained demonstrated-pose probabilities are .3702% and .3314%. BC strongly
raises useful-move likelihood in these states and increases repair-all likelihood;
it does not uniformly raise commit likelihood. This supports a useful-movement
and sequence-learning effect, without proving the cause of each policy decision.
A follow-up should test whether this transfers to other useful moves and factory
geometries, and separately inspect post-pose continuation. No follow-up training
or parameter retuning is included in this screen.

## Audit and operational limits

Both arms have identical initial model, both optimizer, sampler, shuffle and stream
state hashes for each seed. Equal seeds do not imply identical wall-clock features
or later trajectories: untrained seed-29 two-second scores already differ (3.33%
control, 5.00% treatment). The matching action-count results above reduce reliance
on throughput differences, but two seeds remain a small exploratory comparison.

| Audited evidence | PPO | PPO + BC (.05) |
|---|---:|---:|
| On-policy training actions | 32,768 | 32,768 |
| Exact learner/Adam/RNG update replays | 128 | 128 |
| Native snapshot restorations | 1,024 | 1,024 |
| Continuing-boundary / bootstrap checks | 896 | 896 |
| Ordinary timed evaluation cases | 48 | 48 |
| Charged ordinary evaluation actions | 87,024 | 83,419 |
| Commit/final witness references | 21,803 | 24,225 |
| Unique native-admitted witnesses within arm | 471 | 625 |
| Held-out completion cases, initial and final | 64 | 64 |
| Held-out completion policy actions | 4,096 | 4,096 |

Completion audits additionally recompute every sampled action from its recorded
observation and sampling RNG, check all demonstrated-action likelihoods and
independently admit 47 / 56 distinct completion witnesses. The full branch and
output audits preserve the frozen directed topology, endpoint identities and rates.

Control and treatment runs take 262.69 and 267.03 seconds. BC adds 42,912 sampled
teacher-label occurrences, repeating the 33 training labels. Mean learner-update
time rises from about .241 to .282 seconds. BC triggers more KL stopping: actor
epoch totals are 250/249 in control and 210/237 in treatment for seeds 17/29.
Every critic still receives four head epochs per update. Thus on-policy action
budgets match; supervised compute and actual actor step counts differ.

The two-second limit is a native acceptance budget, not a strict process-return
latency. Maximum observed elapsed values are 2324.39 ms and 2316.42 ms. Delayed
guard detections are checked to reject without charging an action or changing
the draft revision or retained best. Full reports retain actual elapsed time.

Figures and the machine summary are retained under
`artifacts/factory-edit-pilot/blueprint-bc-comparison-20260911/`:
`comparison.png`, `completion.png`, `policy-improvement.png`, and `summary.json`.
The [compact provenance manifest](2026-09-11-blueprint-ppo-bc-provenance.json)
binds the selected evidence and source files.

## Question and fixed protocol

The post-pose census found policy-created states that existing routing could turn
into smaller valid factories, including an original-start C1 move from area 700
to a recoverable 665 followed immediately by `restore_best`. The user approved a
small supervised component to help finish useful moves.

The [registered contract](../designs/2026-09-11-blueprint-ppo-bc.md) compares PPO
alone with PPO plus .05 times demonstration negative log probability. Each arm
uses fresh seeds 17 and 29, 64 updates, eight persistent streams, 32 actions per
chunk and 256 actions per episode: 32,768 on-policy actions per arm. The architecture,
kind-balanced action distribution, reward, starts, routing bounds, learning rate
3e-4 and other PPO settings are identical. Teacher transitions do not enter PPO
ratios, GAE, native rewards or critic targets. Each actor-active minibatch samples
eight complete three-action demonstrations uniformly with replacement. BC stops
with the actor when the KL guard triggers; the critic-only tail continues.

Ordinary policy evaluation uses updates 0/16/32/64, original/overlap/improved starts,
two sampling streams per start and two seconds of native session time. It also
records quality at action milestones. Teacher completion is never called during
policy evaluation. A separate held-out draft evaluation starts before or after
the useful move, then gives the policy 64 actions and a fresh 2000 ms operational
allowance. The post-pose setup move is explicit, charged and outside policy actions.

The experimental learner/auditor has an explicit BC entry point. Historical PPO
runtime files remain source-identical so prior checkpoint and forensic tools can
still verify their frozen protocols.

## Demonstration selection and scope

All 46 area-improving post-pose cases from the audited *training* census were
considered. Seventeen supplied-compaction starts were excluded. Both the pre-pose
`repair_all, commit` branch and `pose, repair_all, commit` branch were restored from
the authenticated original checkpoint, retaining its clock, counters and routing
bounds. Six further moves did not reduce area relative to the recoverable pre-pose
draft. No remaining candidate had an inconclusive failed branch.

The 23 accepted source cases reduce to 15 distinct physical-draft/move examples.
Grouping by the physical pre-pose draft and retained best gives 12 groups. The
registered sorted-hash split assigns nine groups / 11 demonstrations / 33 labels
to training, and three groups / four demonstrations / 12 labels to holdout. An
additional check finds no cross-split physical policy input at any of the three
label stages, ignoring clocks/counters. Every retained branch is independently
replayed and its witness admitted by the complete native factory validator.

All 15 useful moves translate C1 upward by one or two tiles; rotation stays fixed.
This is a narrow within-fixture sequence-learning screen. The four held-out
examples are correlated and cover only three pre-pose groups. Neither their
completion counts nor ordinary evaluation on the same five-facility fixture
establish generalization to other factories or arbitrary overlap recovery.

## Verification completed before the screen

- Coefficient zero reproduces the original PPO update exactly.
- BC reaches the actor/shared encoder and has no direct critic-head gradient.
- Forced KL stopping allows one actor epoch and four critic epochs; BC draws stop
  with the actor, whose parameters and Adam state stay fixed through the tail.
- A second update reconstructs exactly from nonempty model/Adam/RNG state.
- Dataset replay, native readiness, readiness update audit, and actual CLI resume
  from a copied nonempty checkpoint pass.
- Workspace check passes; Rust tests: 563 passed, 5 ignored. Python tests: 384 run,
  7 skipped. No native implementation or dependency changed in this experiment.

## Reproduction

Use the retained source runs named by `tools/factory-edit-pilot/blueprint-ppo-bc.json`.
They contain non-replayable training observations and native checkpoints; a seed
alone is insufficient. The generated dataset and native library are bound by SHA.
All generated outputs live under ignored `artifacts/factory-edit-pilot/`.

```sh
P=tools/factory-edit-pilot/.venv/bin/python
$P tools/factory-edit-pilot/build_blueprint_bc_data.py --config tools/factory-edit-pilot/blueprint-ppo-bc.json --output artifacts/factory-edit-pilot/blueprint-bc-data-20260911
$P tools/factory-edit-pilot/audit_blueprint_bc_data.py --dataset artifacts/factory-edit-pilot/blueprint-bc-data-20260911/dataset.pt.gz
$P tools/factory-edit-pilot/blueprint_bc.py train --config tools/factory-edit-pilot/blueprint-ppo-bc.json --native-library target/release/libaic_pilot_native.dylib --output artifacts/factory-edit-pilot/blueprint-bc-control-20260911 --coefficient 0
$P tools/factory-edit-pilot/blueprint_bc.py train --config tools/factory-edit-pilot/blueprint-ppo-bc.json --native-library target/release/libaic_pilot_native.dylib --output artifacts/factory-edit-pilot/blueprint-bc-treatment-20260911 --coefficient 0.05
```

Fresh output directories are required. For a repeated run, use new arm directories;
to rebuild the dataset elsewhere, update its external config path before rebuilding.
Audit each arm with `audit_blueprint_bc.py --run ARM`, then run
`evaluate_blueprint_bc_completion.py --run ARM` and
`audit_blueprint_bc_completion.py --run ARM`. Compare with
`summarize_blueprint_bc.py --control CONTROL --treatment TREATMENT --output OUTPUT`.
`blueprint_bc.py resume --output ARM` resumes the last durable learner checkpoint;
retained observations support exact learner replay, not identical future sampling.
