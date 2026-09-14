# Quality-weighted replay learning study

Status: withdrawn from the requested from-scratch comparison; execution stopped
at the user's direction. This was a trained-checkpoint transition study, not a
comparison of methods applied from initialization. No final winner was produced.
The previous coverage-gated experiment's scientific findings and recommendation
are withdrawn and are not evidence for this study.

## Question and registered comparison

Does allocating more learning to better successful edits improve the policy's
actual native-valid layouts? The [accepted design](../designs/2026-09-14-quality-weighted-replay-study.md)
separates success quality, actual learning allocation and held-out policy outcomes.
[Plan 08](../plans/2026-09-14.08-quality-replay-learning-study.md) implements the
study; [plan 07 readiness](2026-09-14-quality-replay-execution-readiness.md)
verifies the retained learner/optimizer/RNG/replay and normal execution paths.

Eight profiles train for 32 new normal updates from the newly paused main
checkpoint at update 1908. Native rules, generator, 80/10/10 curriculum, model,
optimizer and SIL coefficient remain unchanged. Only the declared replay
sampling/count/retention contrast changes. Normal PPO early stopping can produce
different actual minibatch counts; report actual draws and costs, not just the
nominal per-minibatch sample count.

Two new panels each contain 32 geometries, 16 per size stratum, and two action
replicates per geometry. Development selected its cohort from 96 generation
attempts; confirmation used 56. Selection uses native generation validity and
identity disjointness, never editing outcomes. Both panels reject overlap with
the parent, prior panels, each other and actual new training inputs. Overlapped
factories cannot be silently removed from reported means.

The two highest-scoring nonbaseline development profiles proceed to confirmation,
even when below baseline. An immutable receipt fixes this selection before any
confirmation evaluation. The baseline and both finalists each receive two
stochastic training replicates ending at parent+96: a continuation of the first
32 updates and an explicit paired action/shuffle RNG branch from the original
parent. Fresh slot files are paired when used; actual inherited starts remain
arm-specific. These are correlated replicates of one parent, not independent
training initializations.

## Results

All eight development arms completed 32 actual updates each. Their new-panel
mean validated area reductions were:

| Profile | Development area reduction |
| --- | ---: |
| baseline-8 | 37.0872% |
| uniform-16 | 37.1870% |
| uniform-32 | 34.3616% |
| gain-8 | 36.1596% |
| score-8 | 38.9065% |
| geometry-8 | 38.5305% |
| priority-8 | 37.2362% |
| protected-score-8 | 37.3869% |

The immutable selection receipt chose **score-8** and **geometry-8** before
confirmation evaluation. Receipt SHA-256:
`7f51cf32cfec525c307e79af0ec3aef60c2fecb29776d8d087631db112d09c1b`.
The baseline was registered to confirm. Its first 96-update endpoint scored
35.0589% on the confirmation panel. Its second replicate stopped after 28 updates
when the user changed the experiment to training from scratch. No candidate
confirmation completed. These development scores and the shortlist cannot be used
as evidence or selection inputs for the replacement study.

The gain-only arm raised mean physical success gain per draw from 0.04977 to
0.05383, about 8.2%, while its development policy quality was lower. Increased
physical-quality allocation and improved policy performance are distinct claims.
The 16/32-count arms made 28,928/62,976 actual sequence draws versus 15,488 for
baseline. Their development differences were +0.10 and -2.73 percentage points,
respectively. These observations are not exclusion thresholds or final adoption
evidence.

No coverage saturation or arbitrary minimum improvement excludes a trained arm.
The registered final ranking would have used actual confirmation area reduction, averaged over
action replicates within geometry, geometries within stratum, both strata equally,
and the two training replicates. Whole-geometry stratified bootstrap intervals
keep both action/training replicates attached and describe panel variability
conditional on this parent and these training runs.

## Execution and verification

The experiment uses the same verified normal CUDA learner, packed batches,
persistent workers, compact replay publication and checkpoint retention. Added
study observation does not change the learner transition. Native-quality metadata
is populated at initial import or true terminal admission and reused. Allocation
records distinguish physical native gain from candidate scores and raw/corrected
SIL contributions; a larger loss is not evidence of better policy quality.

The storage guard counts new inode versions once, ignoring existing parent
hardlinks. It conservatively retains charges after deletion, uses exact normal
publication paths and first-access fresh-slot directories, and reconciles full
trees between arms. Final current owned bytes are also reported separately.
No per-update full-tree scans, duplicate checkpoints or full-rollout archives are
introduced. The full study has 736 new updates, 14,400 seconds, 12 GiB added
artifacts, 48 GiB process-tree RSS and a 16 GiB disk reserve as hard ceilings.

All 21 focused quality tests passed locally with the release native library and
on the server without skips. They cover native observer equivalence, actual
frequency/contribution measurement, priority continuation, paired RNG changes,
geometry/stratum/replicate aggregation, bootstrap pairing, selection below baseline,
quantity/equality ties, invalid/duplicated evaluation rejection, source/selection
binding, initial-evaluation isolation, conflicting fresh slots and hardlink charging.
Workspace check/tests and artifact hygiene passed. The experiment was gracefully
stopped after 348 normal updates in 4,603.30 seconds. The final child preserved
its complete-wave checkpoint at update 1936 and reported `paused / requested_stop`.
The orchestrator recorded `incomplete` with no ranking. All experiment processes
exited, and both main state/job hashes remained unchanged. Main remains paused;
there is no adoption or restart.

`artifacts/study/withdrawal.json` records the user-directed withdrawal separately
from the unmodified execution receipt. It binds the preserved checkpoint and
execution result. The original model/optimizer/replay, shortlist and both panels
are excluded from the replacement scientific comparison.

## Reproduction and retained evidence

The fixed protocol is
[`declarative-quality-study.json`](../../tools/factory-edit-pilot/operations/declarative-quality-study.json).
The execution source is frozen under
`/home/ashrimp/Srv/aic-quality-learning-20260914`; artifacts are under its
`artifacts/study/` directory. Each branch retains a normal report, compact quality
summary, native-valid evaluation, actual inputs and source-bound checkpoint.

```sh
python tools/factory-edit-pilot/operations/declarative_quality_study.py run \
  --parent /home/ashrimp/Srv/aic-declarative-efficiency-20260913/artifacts/resumed-four-hour \
  --parent-source /home/ashrimp/Srv/aic-declarative-storage-20260914 \
  --readiness /home/ashrimp/Srv/aic-quality-learning-20260914/artifacts/readiness.json \
  --output /home/ashrimp/Srv/aic-quality-learning-20260914/artifacts/study \
  --environment /home/ashrimp/Srv/aic-declarative-batch-20260913/.tracking.env
```

This records the actual invocation. The output already exists; do not overwrite
it or rerun the experiment automatically. The server Python environment is
`/home/ashrimp/Srv/aic-ppo-padded-curriculum-12h-20260911/tools/factory-edit-pilot/.venv`.
