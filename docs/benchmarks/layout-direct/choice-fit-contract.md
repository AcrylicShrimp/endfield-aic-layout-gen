# Complete-state direct-choice learnability probe

Status: specified before selection/training, 2026-09-07. Diagnostic imitation only.

## Question, prior evidence and scope

Can the unchanged direct policy fit already executed, independently valid improving actions
on complete factory states, and reproduce those choices through the actual executor?
This separates basic representation/optimization response from the main RL credit experiment.
It does not replace the streaming RL objective, establish a deployment policy, or demonstrate
long-horizon planning. The user's area/density reward suggestion remains unimplemented.

Read-only availability inspection of the completed frozen-batch pilot found 158 distinct
small-factory pre-states with accepted new-incumbent actions that also improve the current
tuple, covering all five action kinds. The target supplies only two such distinct pre-states,
both port actions. No target area-improvement example is available in this source population.
Keep that limitation explicit; do not invent labels or substitute smaller target factories.

## Frozen selection and input DTO

Pin `layout-direct-frozen-batch-01/report.json` at SHA-256
`f1d8099372fd8214da9341355a60766938534479247be33420c2da1ff93ada29`.
Read every referenced behavior capsule and source episode in batch, worker, step order.
Eligible positive: accepted, new incumbent, and the final current tuple strictly improves
the pre-action tuple. Deduplicate by full pre-state witness hash within each root.

For the small factory select twelve states by round-robin over
`relocate, pose, port, reroute, compact`, preserving source order within each kind and
deduplicating pre-states. For the target select its two available states in source order.
Small selection indices 2,5,8,11 and target index 1 are held-out states; the other nine
examples are training. These are related states from two seen factories, not independent
factory generalization. All kinds appear in small training; no split or seed selection by fit.

For each selected state retain the original complete request/graph, source capsule/episode
references, pre-state hash, recorded decisions/action and before/after tuples. Reconstruct
the complete witness, reset the real environment with a one-action budget, and obtain a new
observation. This intentionally resets temporal context; do not call it historical replay.
The reset geometry must hash-match the recorded pre-state. Decode the original decisions
against that observation and require the same action. Execute the teacher and freshly audit
the resulting complete factory; require the recorded after-witness hash and tuple.

When available, keep the first rejected or physically unchanged source action at that same
pre-state as a contrast. Reexecute and audit it from the same reset geometry. Do not label
valid temporary deterioration, mere novelty, or a different pre-state as a negative example.
Missing contrasts remain missing; no new search for favorable negatives.

## Fixed fit and controls

Use the common initial width-32 model and empty Adam state from the pilot's original snapshot.
No architecture, mask, action, data-admission or production code changes. Minimize mean
teacher-forced full-action negative log likelihood on the nine training observations with
Adam lr 0.003, global gradient clip 1, entropy zero, full-batch streaming accumulation.
Run exactly 128 updates, checkpointing at 0,16,32,64,128. No early selection of best checkpoint.
Keep the unchanged initial policy as the no-update control. No negative-example loss term:
contrast likelihoods are diagnostic, not an added penalty for failed actions.

Record per-example joint NLL, stage probabilities/legal support, greedy teacher-prefix match,
and contrast NLL at each fixed checkpoint. Greedy match means every chosen conditional
index is the first argmax on the teacher prefix; it is not beam/global joint-MAP search.
Record optimizer/CPU RNG state, gradient norm and parameter displacement. The first update
must match the existing combined updater with constant +1 signal and zero entropy on the
same behavior-matched examples. Later teacher-forced updates are supervised, not on-policy RL.

At the initial and final snapshots, sample eight fresh actions per selected state, each
from a separate identical one-action reset, using SHA-256-derived
`["choice-fit-v1", state_index, sample_index]` seeds matched across the two models.
Execute and independently validate all 224 sampled proposals. Report exact teacher matches,
accepted/improving/unchanged/rejected outcomes and full quality tuples separately for the
nine training and five held-out states, and separately by root. A valid alternative improvement
is not a teacher match but is still a geometric improvement. Do not infer generalization from
the 112 correlated samples per model. Teacher/contrast verification adds at most 28 actions.

## Output and gate

Exclusive output directory; version-1 finite JSON dataset, provenance, checkpoint/optimizer
snapshots, learning curves, all teacher/contrast/sample episode references and fresh audits.
Existing per-capsule 512 MiB guard applies. Structured failures use `choice-fit-failed`,
severity `error`, stage `complete-state-choice-fit`, with message and relevant input/state.
Reject changed source hashes, unavailable selected positives, changed legal decisions,
nonreproducing teacher geometry, incomplete factory audits, or nonfinite updates. No fallback.

A narrow positive training-fit gate requires final mean training NLL <= 1 and at least 8/9
greedy teacher matches. Report the actual sampled-action execution response regardless of
that gate. A failed finite fit is not proof that this architecture cannot represent the task;
a passed fit is not proof that current RL credit learns it, that held-out states generalize,
or that the 59-facility target can be compacted. Inspect actual geometry before making any
packing-quality claim. Follow-up interventions must use these findings, not silently change
the user's objective or the retained area/tiles/turns ordering.
