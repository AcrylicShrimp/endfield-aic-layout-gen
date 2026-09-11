# Persistent blueprint PPO: first learning screen

The registered two-seed experiment completed, but the final learned policies did
not outperform their untrained checkpoints on aggregate timed area reduction.
Persistent native drafts, continuing credit and durable learner state are now
implemented; this result does not establish a useful long-horizon optimizer.

Implementation: `96ab815`. Configuration and interpretation were registered in
[the experiment contract](../designs/2026-09-11-blueprint-persistent-ppo.md).
The retained local run is
`artifacts/factory-edit-pilot/blueprint-ppo-20260911`.

## Measured quality

Each entry is the mean policy area reduction over two sampling streams with a
2-second native budget. Reduction is relative to the post-prefix valid best,
so the already-improved start's planted 15% reduction is excluded.

| Seed | Evaluation start | Untrained | Update 64 |
| --- | --- | ---: | ---: |
| 17 | Original | 2.50% | 0.00% |
| 17 | Overlap | 12.50% | 0.00% |
| 17 | Already improved | 2.94% | 0.00% |
| 29 | Original | 2.50% | 10.00% |
| 29 | Overlap | 12.50% | 0.00% |
| 29 | Already improved | 0.00% | 0.00% |

Across the six cases per seed, mean reduction changed from **5.98% to 0%** for
seed 17 and **5.00% to 3.33%** for seed 29. Across both seeds it changed from
5.49% to 1.67%. Seed 29 did improve the original-start stratum: both final
samples reached area 630 from 700. It lost the untrained policy's overlap-start
improvements. The trained already-improved starts retained area 595.

At updates 0/16/32/64, mean reductions were 5.98/0/0/0% for seed 17 and
5.00/6.67/4.17/3.33% for seed 29. Selecting the intermediate peak after seeing
these results would not establish a reliable model-selection rule.

For the 12 matched final-versus-untrained cases, area alone improved in two,
tied in four and worsened in six. On the full lexicographic score, there were
two wins, one tie and nine losses. Equal area can still differ in physical
transport tiles or route turns. These two-seed, single-fixture repeated cases
are diagnostic observations, not a generalization or significance claim.

## Learning behavior

Both seeds completed 64 updates, 64 real episodes and 16,384 policy actions:
128 updates, 128 episodes and 32,768 actions in total. A 256-action native
episode persisted across eight 32-action rollout chunks. The run took 258.46
seconds including periodic evaluations and retained evidence I/O.

The sampled pose share in the timed evaluation dropped from 21.8% to 0.7% for
seed 17 and from 21.2% to 1.7% for seed 29. The sampled pose candidates change
coordinates while retaining rotation; the native API additionally supports
manual rotations. The final policies mostly selected repairs, commits and restore-best.
This is evidence of reduced placement exploration; the run does not isolate its
cause. Actor KL stopping alone is not an adequate explanation: seed 29 used all
four actor epochs at every update and showed the same direction of change.

Successful score improvements are distinct from positive area reward. Seed 17
made 140 score-improving training commits and seed 29 made 77, while cumulative
area rewards were only .60 and .70. Reward is the decrease in retained best area
divided by original area; unchanged-area improvements in routes receive zero.

Only ten training commits improved area: four in seed 17 and six in seed 29.
Seven used the explicit repair-ready compaction start. Those are evidence of
finishing a supplied opportunity, not discovering that pose. The other three
were seed 29's overlap/original-start improvements at episode actions 88, 72
and 126. Four positive area commits occurred after action 8; three occurred
after actions 32 and 64. Long drafts can therefore produce a valid later gain,
but this does not show that learning improved their frequency or final quality.

Across training, 15,847 repair actions included 504 actions that actually reduced
pending connections, repairing 1,098 original connections in total; 333 actions
cleared the remaining pending set. These counts exclude restore-best, which was
selected 6,780 times. Pending connections remained after 5,924 actions (18.08%),
and overlap remained after 3,355 actions (10.24%). The policy was permitted to
work through unfinished drafts; it did not have to validate after every action.

## Contract and verification

The graph input carries the immutable original directed blueprint and the live
private draft. Facility overlap and pending original connections survive learner
updates. Only complete physical validation can export a draft or replace the
retained best. The new native checkpoint stores this state directly, so resume
no longer has to reconstruct an unfinished edit from a final factory witness.
Single and packed graph execution share one implementation; no old PPO
observation/checkpoint adapter was added.

The readiness job verified true-terminal versus nonterminal GAE, packed graph
outputs/gradients, and a second update reconstructed from nonempty Adam states.
Its forced KL threshold exercised a critic-head-only tail while the actor,
shared encoder and actor Adam state stayed frozen. A separate CLI resume test
started from a copy of the first durable readiness update, completed the next
chunk and final evaluation, and passed the same independent audit. The original
readiness evidence was preserved.

Verification before the main run: `cargo fmt --all`, `cargo check --workspace`,
`cargo test --workspace` (563 passed, five ignored), and the Python suite (379
tests run, seven skipped, no failures). Artifact policy checks passed. The main
training and evaluation used the frozen copied release native library on CPU
with one Torch thread and deterministic tensor operations.

Native time includes prefix setup, observation, inference and action work;
initial complete-factory admission is measured separately. Training time also
charges learner work while a draft stays open. Session resume retains prior
elapsed charge and excludes time offline. The exact audit replays saved learner
updates, not future wall-clock observations or action streams. Matched sampling
seeds do not imply identical trajectories under changed weights and timing.

All 48 evaluations hit the elapsed guard rather than the action cap. Observed
return times ranged from 2.0003 to 2.3028 seconds: synchronous Python work can
delay the next native guard check. The native API rejects actions that start or
finish after its deadline. All 17 cases returning more than 10 ms late ended
with a rejected call that charged no action and changed neither draft revision
nor best. The 2-second budget is an acceptance deadline,
not a hard process-return latency guarantee. Trace retention and runtime timing
also limit interpretation of these measurements as production throughput.

The final independent audit passed all 128 learner-update reconstructions,
1,024 actual native snapshot restorations, 896 continuing physical boundaries
and nonterminal bootstraps, and 128 true-terminal frontiers. It recomputed the
recorded policy probabilities/values in all 48 timed evaluations (84,950 charged
policy actions). All 21,445 retained commit/final witness references passed the
directed-topology audit; all 426 unique witnesses also passed separate public
complete-factory admission. There were no invalid factory exports. The 2,503
rejected training actions were normal private-edit outcomes, not exported
invalid factories or whole-request infeasibility claims.

## Reproduction and retained evidence

Use the build instructions in the [pilot README](../../tools/factory-edit-pilot/README.md),
then select `AIC_NATIVE_LIBRARY` for the current platform:

```sh
tools/factory-edit-pilot/.venv/bin/python tools/factory-edit-pilot/blueprint_ppo.py train --config tools/factory-edit-pilot/blueprint-ppo.json --native-library "$AIC_NATIVE_LIBRARY" --output artifacts/factory-edit-pilot/blueprint-reproduction
tools/factory-edit-pilot/.venv/bin/python tools/factory-edit-pilot/audit_blueprint_ppo.py --run artifacts/factory-edit-pilot/blueprint-reproduction
tools/factory-edit-pilot/.venv/bin/python tools/factory-edit-pilot/summarize_blueprint_ppo.py --run artifacts/factory-edit-pilot/blueprint-reproduction
```

The retained run contains `protocol.json`, `state.json`, `audit.json`,
`summary.json`, `learning.png`, `evaluation.png` and `first-improvement.png`.
Every rollout chunk retains actual native observations and snapshots. Every
timed evaluation retains its trace and final best. Model/optimizer checkpoints
0/1/2/16/32/64 are retained for each seed; the other regenerable optimizer
snapshots were removed after completion. All generated outputs remain ignored
under `artifacts/`; they are local evidence, not a backup. A compact tracked
[provenance manifest](2026-09-11-blueprint-persistent-ppo-provenance.json) binds
the protocol, state, audit, summaries and retained checkpoints.

## Research decision

The persistent-draft contract passed its mechanical checks. The observed policy
quality does not justify treating this PPO setup as an effective optimizer.
The next bounded question is why placement exploration disappears: inspect the
advantages assigned to pose actions against subsequent repairs/commits, and
separate sparse true area reward from critic/bootstrap error. The present run
cannot establish which cause dominates. Changing reward, curriculum, horizon
or architecture simultaneously would obscure that question.

## Follow-up: recorded credit diagnosis

The subsequent read-only diagnosis used every recorded training chunk without
changing the learner or collecting new experience. Run:

```sh
tools/factory-edit-pilot/.venv/bin/python tools/factory-edit-pilot/diagnose_blueprint_credit.py --run artifacts/factory-edit-pilot/blueprint-ppo-20260911
```

Of 128 batches, 119 had zero sampled area reward. Their GAE still contained
value predictions and, except at true terminals, a bootstrap. The learner
normalizes each batch's advantages to unit standard deviation. Consequently,
small value-derived differences can drive policy updates even when that batch
contains no observed improvement. This is the implemented PPO rule, not a
newly discovered sign or terminal-mask bug. Its usefulness depends on the
critic predicting the relevant future gains accurately.

Seed 17's first batch already assigned pose actions a mean normalized advantage
of -0.252; 40 of its 53 poses (75.5%) had negative normalized advantage. The
second batch had no positive reward and its pose mean was -0.305. Across the
first eight batches, the pose mean was -0.283 versus +0.054 for commit. Pose
samples fell from 53 to six per 256-sample batch. Seed 29 initially had a
positive pose mean (+0.132 in batch one) and retained exploration longer.
This is direct evidence that the recorded policy objective often discouraged
movement, though shared-encoder value gradients and later Adam steps prevent
attributing the entire probability change to this statistic alone.

Environment persistence does not make GAE span the whole 256-action episode.
Its direct sampled-reward contribution stops at the 32-action chunk boundary;
later reward is represented by the frozen successor value. Within a chunk,
lambda .95 also attenuates a reward 16 actions away to about 44% of its direct
contribution. In the realized trajectories, 58 poses preceded a later positive
reward in the same episode. For 45 of them, that reward lay outside their
sampled chunk, leaving zero direct sampled-reward contribution. Thirty-two of
the 58 received negative normalized advantage. A pose preceding a reward need
not have caused it, so these counts do not prove those labels were incorrect.
They identify how heavily learning these trajectories relies on the critic.

All four seed-17 area improvements came from supplied repair-ready starts;
none followed a policy pose in that episode. Seven of the ten total area gains
came from that start. The observed successful examples therefore emphasize
repair/commit much more than policy-discovered movement followed by recovery.

On one identical frozen original-state input per seed, pose probability changed
from 22.01% to 0.85% for seed 17 and from 20.49% to 1.58% for seed 29 between
checkpoints 0 and 64, setting elapsed fraction to zero. Setting only elapsed
fraction to one changed final probabilities to 0.86% and 1.60%. Thus elapsed
feature extrapolation does not explain the suppression on these fixed states;
this narrow probe does not rule out all training/evaluation distribution shifts.

The leading hypothesis is a feedback loop between rare movement-to-reward
examples, critic-dependent delayed credit and reduced movement exploration.
This evidence motivates a controlled credit comparison before another long
training run; it does not isolate one cause experimentally. The earlier prose
describing sampled pose as movement plus rotation was corrected above: rotations
were available to manual native actions, not this policy's candidate generator.
