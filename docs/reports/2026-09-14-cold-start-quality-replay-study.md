# Cold-start quality replay comparison

Status: all eight development arms complete; two-initialization confirmation running with a user-authorized one-hour clock extension.

## Question and registered comparison

Does changing success replay sampling and retention improve early policy learning
when every arm begins with random weights and empty learned state?

The [accepted design](../designs/2026-09-14-cold-start-quality-replay-study.md)
registers eight profiles, 128 updates each under initialization seed 71, selection
of the two best nonbaseline development endpoints, and confirmation of those two
plus baseline. Confirmation combines read-only evaluation of their seed-71
128-update endpoints with three newly initialized seed-113 128-update runs.
The comparison allocates 1,408 scientific updates, including the 86 valid updates
retained from the second attempt. Continuation needs 1,322 new updates. The first
cold attempt consumed 128 discarded updates, for a total ceiling of 1,536. The main
update-1908 run stays paused.

The former warm study is withdrawn as evidence for this question. None of its
learned state, priorities, input bank, panels, outcomes or shortlist initializes
or selects these arms. Numerical architecture and optimized execution code are
reused. The cold configuration is registered in
[`declarative-quality-study.json`](../../tools/factory-edit-pilot/operations/declarative-quality-study.json).

Both initializations pair actual fresh generated files by canonical slot, while
inherited starts come from each arm's own previous wave. Development and
confirmation each use 32 new geometries, 16 per size stratum, and two action seeds
per geometry. The replacement excluded all five prior panels and reserved both new panels
before training admission. Policies apply from update 1 with no shared learned warmup.

Selection uses actual native development area reduction at update 128. It has no
coverage, saturation or minimum-gain gate. Confirmation remains inaccessible until
an immutable selection receipt exists. Final uncertainty uses 10,000 paired,
stratified whole-geometry resamples with both observed initializations attached;
it does not estimate broad initialization variance or convergence.

## First-attempt failure and correction

The first cold attempt stopped automatically after baseline update 128 and
1,484.149 seconds. Four development geometries (five training slots) and eight
confirmation geometries (22 slots) overlapped actual fresh training inputs.
Different generation seeds can produce identical physical geometry. Rate changes
are intentionally excluded from the established geometry identity. Every overlap
was a fresh start; no candidate was selected. Its quality curves are invalid as
held-out evidence and cannot select or initialize the replacement.

The corrected run reserves both new panels before any training admission. A
reserved native-valid generated candidate consumes an ordinary bounded attempt,
then retries before blueprint editing or policy actions. In that correction, actual inherited geometry
was checked too, including restoration, and failed closed on a collision. Donor
selection, quotas and fallback rules were unchanged. The selected-slot replacement
below supersedes that stop behavior. Existing closeout overlap
rejection remains. All five earlier panels are excluded by identity.

The first checkpoint and leakage closeout remain in the original frozen server
root. Its 1,315,338,556 retained artifact bytes are charged before the replacement
readiness or study writes. No learned state or old input bank is imported.

## Verified execution path

Initial cold implementation: `992cf74`; input-reservation correction: `65e2b6b`. The correction passed
its second independent proposal review (overall round 4), PASS_WITH_NOTES. The cold launch validates a registered configuration family and source
receipt, publishes a complete empty initializer atomically, and rejects warm
receipts or banks. Normal packed CUDA training, persistent workers, compact
publication and checkpoint recovery remain in use.

- Corrected local and server checks: 39 passed, including fresh/inherited reservation rejection, restore, pairing, deadline and persisted cost contracts.
- Identical-input CUDA transitions: all eight profiles passed in 5.81 seconds,
  comparing ordinary, restored and observed restored updates with nonempty Adam.
- Integrated cold readiness: 76.067 seconds for five actual updates, comprising
  0→4 plus restoration of saved live update 1 and one valid update through 2.
- All 32 live streams restored their saved semantic/input state; elapsed native
  time stayed monotonic. Initial model, optimizer, RNG and empty buffer proofs,
  compact publication and all needed live/replay rows passed.
- Readiness used 133,185,230 actual unique added bytes. Its data and state are
  excluded from the scientific run. No capacity-fill or eviction gate was applied.
- Actual readiness rejected 15 reserved candidates across 78 generation attempts
  for 61 fresh slots, before admitting training inputs.
- Workspace check/tests and tracked-file hygiene passed before launch.

The CUDA receipt authenticates its unchanged execution/observer core. The cold
readiness receipt separately binds the complete current orchestration source,
native binary, numerical base and physical worker allocation.

## Results and second stop

The corrected baseline stopped at update 86 after 994.839 seconds. Before wave 44,
stream 5 could begin its inherited episode, the new guard detected that its
parent's improved layout matched development panel geometry. This was the
watered-grass case generated with evaluation seed 718200768. The fresh root had
area 105, while its improved inherited layout had area 98. The protocol at that time
required unchanged donor eligibility and therefore failed closed on this collision.
No policy action was performed on the rejected inherited input.

All 1,376 committed input slots are disjoint from the development and confirmation
panels. Fresh generation rejected 36 reserved candidates across 1,200 attempts for
1,146 admitted fresh slots. This verifies the new exclusion mechanism, but it does
not complete the requested eight-method scientific comparison.

At that stop, no replacement arm had reached update 128, no candidate selection
had been written and no confirmation evaluation had occurred. The baseline's partial development means were
28.70% at update 32 and 32.06% at update 64; these are incomplete learning-curve
observations, not endpoint comparisons or evidence favoring a replay method. The
buffer reached capacity around update 50 and recorded 5,130 evictions by update 86.

Total cold scientific work was **214 updates**: 128 from the invalidated first
attempt and 86 from the incomplete replacement. The checkpoint and input/plan
collision evidence are retained. That stopped protocol did not authorize an automatic retry with changed donor
eligibility. The subsequent explicit user decision and reviewed continuation below
supersede this disposition.

## User-authorized replacement and continuation

The user rejected treating an occasional matching geometry as a reason to halt the
whole comparison and authorized replacing the colliding input. Geometry identity
alone is not equivalent to importing learned model state; it excludes rates and
utilizations. The earlier no-retry/donor-freeze disposition is superseded by this
explicit decision.

Keep donor sampling and slot shuffling unchanged. Only a selected inherited slot
whose actual geometry is reserved becomes a fresh slot, preserving its requested
depth and recording the rejected parent. Other slots remain unchanged; fresh
replacement uses the existing bounded canonical input generator.

The same baseline resumes its own valid update-86 checkpoint. Its model, both Adam
states, RNGs, replay buffer and historical records are preserved, with only the
pending stream-5 start and launch identity amended. All 1,376 committed inputs
were disjoint; the new rule cannot change that earlier prefix. Both original panels
and 1,172 complete generated input folders remain paired across methods. Other
profiles still begin from random weights and empty learned state.

A fresh proposal review passed with notes. Local and server affected checks passed
(33 quality plus five curriculum), as did workspace check/tests. New integrated
readiness passed in 76.160 seconds with five updates and 32 live stream restores.
The actual continuation advanced successfully from 86 through 88, while original
launch/job/checkpoint/origin/study/closeout hashes stayed unchanged. Immutable files
use hardlinks; mutable controls, logs and tracking state are separate.

## Completed development comparison

All eight seed-71 arms reached update 128 with the same random initial policy
(development score 2.58488%). Scores below are native area reduction percentages,
with equal size-stratum weight. No development endpoint was shortened or repeated.

| Profile | Update 32 | 64 | 96 | 128 | Small at 128 | Large at 128 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| baseline-8 | 28.70 | 32.06 | 31.94 | 33.00 | 48.99 | 17.02 |
| uniform-16 | 27.82 | 32.89 | 31.74 | 32.37 | 45.26 | 19.47 |
| uniform-32 | 30.93 | 31.69 | 31.98 | 28.51 | 38.73 | 18.28 |
| gain-8 | 24.85 | 31.03 | 29.96 | 29.65 | 42.91 | 16.38 |
| score-8 | 26.55 | 31.42 | 30.39 | 31.37 | 46.54 | 16.20 |
| geometry-8 | 20.07 | 26.27 | 32.57 | 31.18 | 44.84 | 17.51 |
| priority-8 | 27.00 | 29.50 | 30.93 | 33.93 | 49.74 | 18.13 |
| protected-score-8 | 27.83 | 31.08 | 29.70 | 31.86 | 47.32 | 16.39 |

The immutable selection chose **priority-8** and **uniform-16**, the two highest
nonbaseline endpoints. Priority exceeded baseline by 0.93060 percentage points
on development; uniform-16 trailed by 0.63768. Uniform-32's stronger early score
did not persist to update 128. These are selection observations, not confirmed
policy improvements.

## Physical quality and learning allocation

The following quantities describe the experiences actually reused over all 128
updates. Draw gain is the sequence's native before/after area reduction, distinct
from the policy's whole-episode held-out score. Pool gain is weighted by eligible
entry-updates; it is not a per-draw time-matched counterfactual. Raw SIL contributions
include stage weights; corrected contributions additionally include the existing
importance and draw-count correction. Loss magnitude is not policy quality.

| Profile | Draws | Draw gain % | Pool gain % | Draws ≥10% gain % | Positive labels % | Raw SIL | Corrected SIL |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| baseline-8 | 63,104 | 4.513 | 4.733 | 10.07 | 73.52 | 25876.6 | 3234.6 |
| uniform-16 | 127,232 | 4.561 | 4.797 | 10.01 | 75.94 | 58187.1 | 3636.7 |
| uniform-32 | 256,512 | 4.513 | 4.743 | 9.73 | 76.10 | 133141.4 | 4160.7 |
| gain-8 | 63,488 | 4.788 | 4.599 | 11.67 | 76.78 | 28697.2 | 3587.1 |
| score-8 | 63,360 | 4.861 | 4.753 | 12.24 | 74.01 | 25558.4 | 3194.8 |
| geometry-8 | 63,744 | 4.840 | 4.785 | 11.97 | 74.60 | 26651.6 | 3331.5 |
| priority-8 | 63,360 | 4.498 | 4.635 | 9.44 | 90.17 | 41133.8 | 2624.1 |
| protected-score-8 | 64,256 | 4.982 | 4.803 | 12.40 | 73.70 | 28571.8 | 3571.5 |

Priority sampling increased the fraction of labels with positive advantage from
73.52% to 90.17%, without increasing mean physical gain of the selected sequence.
The physically weighted variants selected somewhat higher-gain sequences, but
none exceeded baseline at the development endpoint. Increasing replay draws by
roughly 2x/4x increased raw learning contribution; this did not establish a policy
improvement.

| Profile | Buffer full at update | Evictions | Terminal attempts | Terminal gain % | Fresh slots | Reserved fresh rejections |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| baseline-8 | 50 | 10,989 | 15,085 | 4.424 | 1,697 | 62 |
| uniform-16 | 48 | 11,196 | 15,292 | 4.439 | 1,705 | 62 |
| uniform-32 | 50 | 10,492 | 14,588 | 4.503 | 1,702 | 63 |
| gain-8 | 52 | 9,991 | 14,087 | 4.273 | 1,702 | 64 |
| score-8 | 54 | 10,331 | 14,427 | 4.480 | 1,702 | 62 |
| geometry-8 | 58 | 9,542 | 13,638 | 4.410 | 1,703 | 62 |
| priority-8 | 46 | 11,848 | 15,945 | 4.300 | 1,698 | 64 |
| protected-score-8 | 54 | 9,632 | 13,728 | 4.604 | 1,708 | 64 |

Every buffer reached capacity and performed thousands of replacements, so retention
mechanisms were exercised. In protected-score-8, category protection constrained
5,029 of 9,632 eviction decisions; no capacity override occurred. The eviction
reason was lowest observed contribution 1,805 times and insufficient-observation
oldest fallback 7,827 times. Evicted entries were 10–64 updates old (mean 33.646);
48 reached the 64-update protection horizon. Terminal gain summarizes all attempted terminal sequence
admissions, including any duplicate outcomes. Fresh rejection counts describe the
paired slot records seen by each arm; summing them would double-count shared
generation work.

Four selected inherited collisions were replaced during development: baseline
(wave 44, stream 5), uniform-16 (51, 15), score-8 (43, 30), and geometry-8 (60, 25).
The other four profiles needed no inherited replacement. All completed development
cohorts passed the original committed-input disjointness check.

## Runtime and retained evidence

The original continuation used the six-hour absolute deadline, Unix
1789374130.8190546. The user subsequently authorized up to one additional hour
to complete confirmation. The final hard deadline is **1789377730.8190546**
(18:22:10.819 KST), including setup, evaluation and finalization.
Its limits are 1,408 scientific updates (86 carried and 1,322 new),
12 GiB cumulative cold artifacts, 48 GiB
process-tree RSS and 16 GiB disk reserve. The 128 discarded updates and prior
artifact ownership are reported separately; the retained 86 are counted once.
Current readiness and continuation storage carry prior ownership charges and
exclude already-charged immutable hardlink versions from new charges. Successful last-link deletion releases owned storage charges. Command
entry and arm boundaries reconcile the ownership ledger; normal updates do not
scan the study tree or save an extra ledger.

The extension proposal passed independent review with notes. Its separately
bound `finish_cold_quality_study.py` reuses the unchanged source/readiness, original
empty initializer, persistent-worker CLI, fresh bank, initial-evaluation cache,
ledger and statistics. Only the coordinator is suspended while the active seed-113
baseline completes. Its completed 128-update state is then summarized once before
the two remaining profiles start empty under the extended clock. No learned state,
launch identity or checkpoint amendment is involved. Six focused extension checks
passed locally and on the server; workspace check/tests passed. The actual complete
original source fingerprint remained identical after installing the controller.
The actual handoff completed after baseline113 reached 128: its final confirmation
score was 20.28673%, and priority113 began from the same complete empty initializer.
The original coordinator and baseline child both exited. The scientific count at
this boundary is exactly 1,152 updates; the last two arms are still running/pending.

Current frozen server root: `/home/ashrimp/Srv/aic-quality-cold-continue-20260914`.
The source and stopped update-86 evidence remain under
`/home/ashrimp/Srv/aic-quality-cold-start-r2-20260914`.
Original invalidated root: `/home/ashrimp/Srv/aic-quality-cold-start-20260914`.
Its `artifacts/study/leakage-closeout.json` records the overlap and checkpoint.

- `artifacts/readiness/readiness.json`: bounded integrated readiness receipt.
- Original root `artifacts/cuda-proof.json`: reused unchanged-core CUDA evidence.
- `artifacts/study/study.json`: scientific allocation, source, panels and progress.
- `artifacts/study/selection.json`: immutable development selection, when available.
- Per-arm `quality-branch.json`, `quality-summary.json` and ordinary seed job state:
  initialization, measured outcomes/allocation and retained checkpoints.

Local readiness and CUDA receipts are retained under
`artifacts/cold-quality-continuation-20260914/`,
`artifacts/cold-quality-study-r2-20260914/` and the original
`artifacts/cold-quality-study-20260914/`. Regenerable artifacts are ignored rather
than committed; these local/server directories are not backups.

The second-attempt closeout verified all its training workers exited and main
remained paused with unchanged monitor hashes. Those two cold roots retained 2,316,387,958 actual unique
artifact bytes at measurement. The replacement's
`artifacts/study/inherited-stop-closeout.json` records the collision, zero committed
input overlap and retained update-86 checkpoint
`7ad05885b146926b2dbf7a5e11e2e9029a19b4fc5038b3df26de54f583f68cad`.
