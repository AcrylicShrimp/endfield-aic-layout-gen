# Cumulative streaming RL budget curve

Status: preregistered before execution, 2026-09-07.

## Question and scope

Does continuing the same learner beyond 64 optimizer updates improve independently evaluated
factory layouts? The frozen-policy pilot reset the model between single updates and cannot
answer this question. The two historical 64-update runs changed other settings together;
neither rules out insufficient training. This experiment takes priority over the unexecuted
choice-fit diagnostic. Poor early performance is not an efficacy stopping rule.

Use the current complete-factory streaming path, not two fixed diagnostic roots or imitation.
The current constructor cannot admit some parallel internal production components. Its policy
is unchanged and the component-admission proposal remains unapproved. Sample the declared
unstratified `any` stream and record all attrition, actual sizes and target families. This is
conditional on existing construction/observation support, not arbitrary-factory coverage.
Do not crop failed requests, change external inputs to rescue geometry, or substitute saved
small roots. Explicit replay of the complete 59-facility target is training, not a holdout.

## Frozen allocation and inputs

CLI: `budget_curve.py prepare|train|analyze`, with explicit config, output and evidence paths.
`budget-curve.json` schema 1 pins seeds 1901, 2901, 3901 and cumulative checkpoints 64, 256,
1024. Each seed starts once from its own initial model and empty Adam and keeps the same
model/optimizer through all 1024 updates. Four independent 512-action trajectories per update;
6,291,456 training proposals across the three seeds. Every eighth group uses the full target;
the other 896 groups per seed draw fresh eligible requests from runtime catalogs. Hash-derived
training seed namespaces include model seed and group. Evaluation never consumes training RNG.

Keep width 32, Adam learning rate 0.003, norm-1 clipping, entropy weight 0.0001 and
`quality-futility-prefix` credit fixed. Reuse the existing behavior-checked updater, decoder,
pipe-first executor, complete-factory audit and lexicographic area/tiles/turns best retention.
Keep train horizon 512 and evaluation horizon 1024 fixed: a longer evaluation is not longer
training. No architecture, reward, mask, action-domain or admission-policy intervention.

Prepare twelve new eligible validation-partition factories using a fixed generation namespace
before any model training, then freeze their complete root hashes and the explicit target.
Reject overlap between validation target families and fresh training target families. Require
minimum two facilities and existing observation support. Each requested eligible root has at
most 200 generation attempts; exhaustion is a structured failure, never an easier-root fallback.
Preparation and each seed's generation use separate workers/deduplication histories. The
sealed test partition is never accessed. This validation set is development evidence, not an
untouched final release test, and twelve roots do not establish arbitrary-factory coverage.

At checkpoint 0 evaluate the unchanged initial policy and uniform-at-each-conditional-choice
policy. At 64, 256 and 1024 evaluate the current policy without changing it. Every evaluation
uses all thirteen roots and two matched sampling seeds (101, 211), hence 130 episodes per model
seed and 399,360 evaluation proposals overall. Sampling streams do not include checkpoint or
arm, so matched comparisons begin with identical random streams. Roots always reset to the
same full initial witness. Initial and uniform controls receive the same 1024 proposals and
per-action routing bounds, but consumed route work need not match and is reported separately.

## Outputs, continuity and evidence limits

Finite versioned JSON: frozen evaluation roots with provenance, per-attempt generation ledgers,
per-update summaries/gradient diagnostics/model+optimizer snapshot fingerprints, all executed
episode traces and retained complete witnesses, all fresh audits, checkpoint snapshots including
Adam state and CPU RNG, and per-checkpoint paired evaluations. Each update's before fingerprint
must equal the preceding update's after fingerprint, including across evaluation checkpoints.
Require exact declared action counts and successful original-request/geometry audits for both
training and evaluation. Reject truncation rather than rank unequal work as a fair comparison.

Save full replay capsules for updates 1, 64, 256, 1024 only, and verify each capsule immediately
with the existing exact update verifier without changing the live learner or RNG. Other updates
retain executed episodes, exact behavior replay checks during optimization, credit summaries,
gradient hashes and state-continuity hashes, but not every observation/conditional distribution;
do not claim every intermediate update is independently replayable from disk. This selective
instrumentation does not change the training algorithm. Source/config/catalog/binary hashes
are checked at checkpoints and shutdown. Changed sources invalidate the run.

Exclusive output directories, 16 GiB output guard per seed/preparation, 20 GiB minimum free
disk and 24-hour per-command wall guard checked between groups/batches; existing per-action,
episode, worker and 512 MiB capsule guards still apply. No artifact deletion on a guard stop.
Failures emit stable `budget-curve-failed`, severity `error`, stage `cumulative-budget-curve`,
message and evidence directory, and retain the last completed group/checkpoint. A failed
command must not create a success report. No automatic restart, resume or budget extension.

## Analysis and decision

Primary development endpoint at each checkpoint: mean paired lexicographic win minus loss
against the seed-matched initial policy; ties are zero. Report every seed separately and the
three-seed mean/range. Also compare checkpoint 256 versus 64 and 1024 versus 256 using matched
root/sampling seeds. Report uniform comparisons, score component differences, per-root retained
tuples, futile fractions, consumed routing expansions, inference and elapsed time. Report the
target separately (area, full score, futile fraction); never let small-root gains hide target
failure. Aggregate independent unit is the training seed, not action count or correlated
checkpoint episodes. Three seeds are a limited budget study, not precise population inference.

Analyze the fixed final checkpoint, not whichever checkpoint looks best. Report regressions,
plateaus and nonmonotonicity. A rising curve supports further learning-budget investigation in
this exposure; a flat or falling finite curve does not prove global impossibility or rule out
all longer training. Even a positive result leaves breadth, low waste, target <=1% voids,
nonredundant routing, visual audits and production integration as separate release requirements.

Before geometric/readiness claims, render and directly inspect the actual target and fixed
largest-validation-root pair at all checkpoints for the first declared seed, plus the largest
final regression across seeds. Label the latter as post-hoc. Verify full witnesses and common
scale; do not treat a lower area number as proof of clean routing or dense packing.
