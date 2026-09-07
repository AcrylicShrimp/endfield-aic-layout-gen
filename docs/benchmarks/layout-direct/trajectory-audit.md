# Direct-layout RL forensic audit and research gates

Status: forensic measurements completed; causal training experiments below are not executed.
Date: 2026-09-07. No model, reward, executor, generator or production-policy change.

## Decision

The two completed streaming runs demonstrate execution and valid retained factories, not an
effective learning method. Neither an architecture failure nor insufficient training is established.
Pause configuration-changing training and production integration while establishing a diagnostic
baseline. User suggestions and researcher intuitions are hypotheses, not experiment conclusions.

Exploratory runs are useful, but the previous credit experiment jointly changed the credit rule,
learning rate and training horizon. Its report disclosed this confounding correctly; it cannot
identify which change helped or hurt. Product-quality checks alone do not resolve that question.
The disconnected-component constructor finding remains a separate, real data-admission issue;
it does not require resolving construction policy before auditing learning on complete existing
factories. The component-admission proposal is not approved or implemented by this audit.

## Evidence and reproduction boundary

The principal source is `target/layout-direct-credit-01/report.json`, SHA-256
`2ddbf0b990ed3df3d4cedb5daa2fa1415ae6e56e9ea1d5c37578298ab7153778`.
Its initial checkpoint hashes to
`3e7ff4d4f28c00925696be12bfcb5441683e1dd7f243c7f1eac46fe65a1ae0f2`, and final checkpoint to
`8d14ed85c6a71ee971e7f38738e9c81eaf8ff219b4fbaa388ed64b9cf2faa4ed`.
Current `policy.py` hashes to
`3c868aa7fe8aac6d50405ae5b4c92c3f967a09ed49f200b21573169394aae209`, identical to that run's
recorded policy. Commit `f700770` preserves its training implementation. The subsequent
`3d11631` changes sampling/stratification and checkpoint persistence, not its credit functions
or policy-gradient update. Do not rerun the current broader sampling config and call it a
replication of the original credit run.

All 256 training episode artifacts were read and their recorded SHA-256 values checked.
For each four-trajectory group, reconstruct pre-action best tuples from the initial score and
the preceding trace row's `best`; reconstruct pre-action futility from rejected actions and
accepted hashes already in that trajectory's visited set, seeded with its initial parent hash.
Apply `terminal_advantages` to final and reconstructed pre-action four-field tuples. Subtract
the latter rank from the former before each action. This exactly reproduces every group's
recorded nonzero-credit count and every episode's final futile count.

This does not reconstruct historical gradients: the completed run saved initial/final weights,
but no intermediate model checkpoints, sampled decoder decisions, behavior log probabilities,
per-stage distributions or per-trajectory gradients. The newer checkpoint feature cannot recover
those missing historical records. We must not invent a checkpoint learning curve retrospectively.

## What the traces establish

There are 64 optimizer updates, 256 trajectories and 131,072 actions, not 131,072 independent
training examples or optimizer updates. Each fresh factory contributes four correlated-through-
the-root trajectories to one update. There are 56 fresh training roots, all with 2–10 facilities,
plus eight replay groups on the same 59-facility target. The target is not held out.

| Reconstructed credit population | Fresh roots | Target replay | Total |
| --- | ---: | ---: | ---: |
| Executed actions | 114,688 | 16,384 | 131,072 |
| Nonzero credit coefficients | 52,396 | 7,121 | 59,517 |
| Positive coefficients | 24,792 | 3,639 | 28,431 |
| Negative coefficients | 27,604 | 3,482 | 31,086 |
| Positive coefficients on rejected/unchanged/revisited actions | 19,601 | 3,013 | 22,614 |

Positive credit on an individually futile action is not by itself a policy-gradient bug: a
sampled return credits a trajectory, and action-independent baselines do not make it an exact
action-value estimator. These counts demonstrate coarse/noisy attribution, not its gradient
signal-to-noise ratio. The score-function factors are needed for that measurement.

The sum of squared coefficients falls from 72,704 for terminal ranks to approximately
57,158.222 for prefix-subtracted ranks on these same trajectories; it nevertheless increases
in 15 of the 64 individual groups. This is a coefficient second-moment comparison, not proof
that the baseline reduced gradient variance or improved training.

All logged combined gradient norms are finite, ranging from 0.001185901 to 0.021724636 before
the configured norm-1 clipping. Clipping was never active. The hypothesis that this clipping
threshold suppressed these updates is contradicted by the logs. Neither finite gradients nor
29,043 changed parameter elements establishes useful policy learning.

The trained development trajectories remove 14,656 area cells in total: 13,488 within the
first 16 proposals, 421 after proposal 256, and 248 after proposal 512. All twelve trajectories
still obtain a later lexicographic incumbent after proposal 256. Large early gains and smaller
late gains coexist; neither shortening the horizon nor extending training follows automatically.
These are summed geometric changes in trained evaluations, not gains attributable to learning
over the matched untrained controls.

In the target evaluation, the trained policy makes 195 relocations, 210 poses, 215 port changes,
206 reroutes and 198 compactions. The respective novel-valid counts are 2, 2, 162, 2 and 0.
Every compaction and 204 reroutes are unchanged. All 1,024 actions produce zero new incumbents.
Thus 162 of 168 novel states are port changes without any retained-quality gain; novelty must
not be equated with learning to pack. See `worker-0/episode-0000076.json` in the credit run.

## Fixed-state policy probe

As a read-only diagnostic, load the original 13 evaluation roots and each trained evaluation's
retained witness. Reset each through the real `DirectEnvironment` and independent validator,
using a fresh 1,024-step episode, then score the same observations with initial and final
checkpoints. Capture first-stage kind logits from `decode`; no parameter or environment action
is updated. This is 26 reset-state observations, not the historical training distribution or
historical time contexts. Both models receive exactly the same observation in each comparison.

| Mean kind probability, 13 original reset states | Initial | Trained |
| --- | ---: | ---: |
| Relocate | 0.200820 | 0.197344 |
| Pose | 0.200421 | 0.197172 |
| Port | 0.199337 | 0.202136 |
| Reroute | 0.199708 | 0.201014 |
| Compact | 0.199714 | 0.202334 |

Mean kind-distribution total variation is 0.00672465 (maximum 0.00763842); mean
`KL(initial || trained)` is 0.000101055 nats. On the 13 retained-reset states, mean total
variation is 0.00708171 and mean KL is 0.000111680. The target's trained probabilities are
`[0.197248, 0.197011, 0.202205, 0.201157, 0.202379]`.

The operation selector barely changes on this probe and remains almost uniform. This does
not establish that all conditional member/rotation/coordinate/port choices are unchanged, or
that a larger network would fail. Future probes must report each decoder stage and legal
vocabulary size separately; aggregate entropy across differently sized factories is confounded.

## Training-method interpretation

The implemented task term is one on-policy score-function update per group, using the summed
conditional log probability of the sampled direct action. It is not PPO, a learned critic,
or standard reward-minus-peer-mean RLOO. Tournament ranks retain lexicographic pairwise ordering
but discard margin information; futility only breaks exact final-quality ties.

The prefix term is computed strictly before the current action. With independent trajectories
and a frozen behavior policy during collection/replay, it is action-independent conditional on
the joint prefix. Subtracting it therefore has zero expected score-function contribution for
that action. This validates its baseline form, not its effectiveness or minimum variance.

A local reference check enumerated all 256 joint outcomes of four independent two-step binary
trajectories at action probability 0.3. Starting score is `[100,20,4]`; a successful first
action reduces area to 90, a successful second action reduces it to 80, and a failed action
leaves geometry unchanged and increments futility. Applying the actual `credit_signals`
function gives both terminal and prefix expected task ascent gradients of 0.1659 after the
trainer's horizon normalization. A central finite difference of the fixed-opponent objective
with logit displacement 0.00001 gives 0.16589999999903765. This verifies that reference case,
not all implementation paths or optimization convergence.

The terminal tournament compares against current-policy peers treated as fixed opponents for
the update. Distinguish that local preference-improvement objective from differentiating the
expected rank of an entire identically distributed self-play population, whose average rank
is identically zero. Also do not call it a gradient of expected bounding-box area. We need an
explicit objective/estimator contract and finite-state reference checks before claiming the
training method is mathematically aligned with the intended stochastic performance criterion.

`decode` averages entropies of the sampled conditional stages while summing their log
probabilities. That is a local exploration heuristic, not the exact entropy of the joint
autoregressive action distribution. Its observed effect must be measured separately from task
gradients; the small configured scalar alone does not establish that effect.

The model does not consume the visited-state set or cumulative futility used by the reward.
Different histories can therefore be indistinguishable to this feed-forward policy even when
revisiting a state has different reward consequences. This limits history-aware waste avoidance;
it is not proof that a reactive policy cannot improve quality or reduce other forms of waste.

## Causal questions and minimum separating experiments

These are proposed research gates, not claims that the experiments have run. Preserve complete
validated factories, original score priority and all production rules. Small controlled tasks
are diagnostics, not a substituted product scope or a replacement for the dynamic stream.

| Hypothesis | Existing evidence cannot settle | Separating experiment |
| --- | --- | --- |
| Estimator or replay is wrong | Finite gradients/action equality are insufficient | Enumerated tiny stochastic reference: analytical/finite-difference gradient versus expected implemented estimator, then exact replay/log-probability checks for every decoder stage |
| Weak exploration or temporal credit | Positive coefficient count is not useful gradient SNR | Frozen complete roots and matched policy snapshots; independent rollout batches, per-trajectory task gradients, baseline and entropy separation; measure variance and update-direction agreement |
| Model capacity/representation is insufficient | One 29,537-parameter model cannot establish a scaling conclusion | Fit known improving choices on identical complete-state diagnostic data; then capacity sweep with data, objective and environment frozen, reporting both matched interactions and compute |
| Training is insufficient | Only 64 updates and no fixed checkpoint curve | Nested update budgets on one frozen method, deterministic data stream and checkpoint eval set; record train and validation curves rather than comparing unrelated fresh groups |
| Main-path distribution is wrong | Almost no new medium/large exposure | Report requested versus admitted topology/size strata and constructor attrition; fix admission only through separately approved production semantics |
| Useful actions are inaccessible or too rare | Sampled rejection is not local/global impossibility | Replay known valid improvements through this exact decoder/executor, then bounded opportunity/repair diagnostics at fixed states; charge all search work |

Before starting a training comparison, register the hypothesis, competing explanations,
input/source hashes, independent RNG streams (initialization, data order, action sampling,
evaluation), changed factors, controls, budget, primary endpoint, uncertainty procedure and
stopping/selection rules. The current single `seed` couples initialization and training stream;
simply changing it cannot isolate initialization variance from data variance.

For the main loop, retain periodic weights plus optimizer/RNG state, behavior policy identity,
replayable conditional decisions, pre-action observations or lossless reconstruction references,
log probabilities, credit components, per-stage entropy/KL, task/entropy gradient measurements,
futility categories, incumbent timing, repair/validation work, and fixed checkpoint evaluations.
No future historical-gradient claim may exceed what was actually persisted.

Use unchanged initial, conditional-uniform and a declared geometric control under the same
executor; report selected interactions and consumed work separately. Run zero/shuffled-signal
controls with entropy disabled when testing task-gradient causality. First show a repeatable
learning response on complete diagnostic factories, then scale capacity or training budget.
Multiple seeds are a variance pilot, not automatic proof of significance. Pair by factory and
account for related families/seeds instead of treating trajectory steps as independent samples.
Keep development selection separate from the sealed test; do not use target replay to claim
generalization. Report uncertainty, effect sizes and failures, not only the best mean.

## Public references and scope

OpenAI's public [evaluation guidance](https://developers.openai.com/api/docs/guides/evaluation-best-practices)
specifies objectives, representative datasets, metrics and repeated comparisons, and cautions
against impression-based evaluation. Its public
[reinforcement fine-tuning guide](https://developers.openai.com/api/docs/guides/reinforcement-fine-tuning)
describes checkpoint comparisons and checking behavior for overfitting and reward exploitation.
These are public API practices, not evidence of a complete private frontier-training procedure
or a recommendation to move this local learner to an OpenAI API.

[Ahmadian et al., ACL 2024](https://aclanthology.org/2024.acl-long.662/) and the
[RLOO implementation documentation](https://huggingface.co/docs/trl/en/rloo_trainer) are references
for distinguishing REINFORCE-style estimators and leave-one-out reward baselines from our
custom tournament objective. Their language-model results do not establish layout performance.

## Verification and boundaries

Raw training artifact hashes, prefix-credit counts, futile counts, gradient norm range and
checkpoint/policy hashes were checked locally. The fixed-state probability probe revalidated
the 26 complete reset factories. No new optimization run, production mutation, dataset rewrite,
architecture cutover or success claim was made. Existing user-owned edits remain untouched.

Separate read-only reviews examined estimator mathematics and raw trajectory behavior. Their
actionable conclusions above were checked locally: the first run's 334 episode artifacts and
the credit run's 295 artifacts (629 total) all match their hashes and independently recomputed
`mechanicalWaste` fields; the late-area totals and target operator counts reproduce exactly.
These comprise 115,456 and 171,008 training-plus-evaluation actions respectively. The exact
finite-state gradient reference and source inspection independently support the baseline and
entropy interpretation. A review is not a new seed replication or proof of method efficacy.
