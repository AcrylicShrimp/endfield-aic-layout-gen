# Replay balance learns parts of the edit, with limited transfer

All fifteen registered runs completed. Balanced replay plus coefficient 0.50
(B50) produced the strongest anchor completion and the only positive final
geometry-transfer results. **No treatment passed the registered transfer or
known-state progress gate.** This result supports partial learning from stronger
large-example exposure, not promotion or an automatic longer training run.

The [design](../designs/2026-09-13-blueprint-replay-balance-experiment.md) and
[runtime report](2026-09-13-blueprint-replay-runtime.md) define the unchanged
comparison. The [compact provenance](2026-09-13-blueprint-replay-results-provenance.json)
binds the retained inputs, fifteen endpoint/recovery states, autonomous probes
and full numerical report. All raw outputs remain under ignored artifacts.

## Completed comparison

Each centered update-512 parent was forked into C, U05, B05, U50 and B50 for
128 child updates. All runs executed 32,768 actions and 512 true 64-action
episodes: 491,520 training actions and 7,680 episode executions overall.
The same 512 actual new inputs were shared across all arms and learner seeds.
Their products were 123 food, 114 medical, 138 watered-grass and 137 copper-liquid.
These are three learner seeds on one input sequence, not independent input
populations. No arm was dropped, extended or selected by an intermediate score.

C uses only its own replay. U/B arms add the same five known training successes;
U samples uniformly and B samples four large plus four small sequences per
minibatch. The suffix is the imitation coefficient, 0.05 or 0.50. All arms retain
centered GAE, original optimizer state, the graph model, original topology and
physical validation, area-only reward, and the existing routing/action budgets.

## Registered verdicts

Triples below are learner seeds **17 / 29 / 41**, in that order. Transfer values
are percent area reduction averaged over eligible factories and both sampling
streams; small-case differences are percentage points relative to C.

| Arm | Pre-pose target completion (%) | Strict transfer (%) | Improved eligible factories | Pooled small delta (pp) | Small preservation |
| --- | --- | --- | --- | --- | --- |
| C | 0.0 / 0.0 / 0.0 | 0.0000 / 0.0000 / 0.0000 | 0 / 0 / 0 | control | control |
| U05 | 0.0 / 0.0 / 0.0 | 0.0000 / 0.0000 / 0.0000 | 0 / 0 / 0 | -0.4795 | fail |
| B05 | 7.5 / 2.5 / 10.0 | 0.0000 / 0.0000 / 0.0000 | 0 / 0 / 0 | -0.3085 | pass |
| U50 | 0.0 / 0.0 / 0.0 | 0.0000 / 0.0000 / 0.0000 | 0 / 0 / 0 | -0.2324 | pass |
| B50 | 17.5 / 12.5 / 17.5 | 0.1042 / 0.0000 / 0.1958 | 1 / 0 / 2 | +0.1852 | pass |

- Known-state progress requires at least +25 percentage points over C from
  pre-pose states in at least two learner seeds. B50 reaches +17.5/+12.5/+17.5;
  B05 reaches +7.5/+2.5/+10.0. U05 and U50 have no pre-pose target completions.
  All four treatments fail this registered gate.
- B50 beats C's zero transfer mean in seeds 17 and 41 and ties it in seed 29.
  However, only seed 41 improves two eligible factories; seed 17 improves one
  and seed 29 none. It therefore fails the requirement for at least two distinct
  improved factories in each of two seeds. The other arms have zero final transfer.
- The pooled small-case C reduction is 3.5614%, giving a permitted decline of
  0.3561 pp. B05, U50 and B50 pass preservation; U05 fails. B50's pooled +0.1852 pp
  hides seed-level differences of -0.7438/+1.4462/-0.1468 pp. Passing preservation
  is not evidence of a uniform benefit across seeds.

## Geometry transfer and historical continuity

Fifteen of the frozen sixteen large factories remain eligible after comparison
with all 512 actual continuation inputs. Medical case 401000112 has a
rate-independent geometry overlap (not an exact-input overlap) and is excluded
from every arm and checkpoint. The common subset exceeds the required minimum
of twelve. Training inputs were not resampled. No unseen-request claim is made.
All arms' initial transfer reductions, on both the strict subset and all sixteen,
are zero, so each final transfer value below is also its own-initial delta.

The historical panel has fifteen small and seventeen large factories. Its late
window averages updates 100/104/.../128. Initial historical small reductions are
5.4137/2.0256/5.6593% for seeds 17/29/41, identical across their five arms;
initial historical large reductions are zero. The panel's known geometry overlap
makes it a continuity measurement, not the primary transfer estimate.

| Seed | Arm | Strict final (%) | All-16 final (%) | Late small (%) | Small vs own initial (pp) | Late large (%) |
| --- | --- | --- | --- | --- | --- | --- |
| 17 | C | 0.0000 | 0.0000 | 4.4529 | -0.9608 | 0.0000 |
| 17 | U05 | 0.0000 | 0.0000 | 3.8888 | -1.5249 | 0.0000 |
| 17 | B05 | 0.0000 | 0.0000 | 3.6351 | -1.7787 | 0.0306 |
| 17 | U50 | 0.0000 | 0.0000 | 4.2236 | -1.1902 | 0.0000 |
| 17 | B50 | 0.1042 | 0.0977 | 3.7091 | -1.7046 | 0.1021 |
| 29 | C | 0.0000 | 0.0000 | 2.6460 | +0.6203 | 0.0000 |
| 29 | U05 | 0.0000 | 0.0000 | 2.1177 | +0.0920 | 0.0000 |
| 29 | B05 | 0.0000 | 0.0000 | 3.1075 | +1.0818 | 0.0000 |
| 29 | U50 | 0.0000 | 0.0000 | 2.6936 | +0.6679 | 0.0000 |
| 29 | B50 | 0.0000 | 0.0000 | 4.0921 | +2.0665 | 0.0087 |
| 41 | C | 0.0000 | 0.0000 | 3.5852 | -2.0741 | 0.0000 |
| 41 | U05 | 0.0000 | 0.0000 | 3.2391 | -2.4202 | 0.0000 |
| 41 | B05 | 0.0000 | 0.0000 | 3.0161 | -2.6432 | 0.0000 |
| 41 | U50 | 0.0000 | 0.0000 | 3.0698 | -2.5895 | 0.0000 |
| 41 | B50 | 0.1958 | 0.1836 | 3.4384 | -2.2210 | 0.0463 |

The nonzero final transfer factory means are B50/17 case 401000056: 1.5625%;
B50/41 case 401000009: 0.8065%; and B50/41 case 401000056: 2.1307%.
Both distinct factories are food factories. These means include both fixed
sampling streams, including unchanged outputs. Four individual evaluation
trajectories contain validated improvements: used areas 3520 to 3410 (seed 17),
3534 to 3477, 3520 to 3480 and 3520 to 3410 (seed 41).
Every other eligible final factory/arm/seed mean is zero. The excluded medical
case also has zero final improvement, so including all sixteen dilutes B50's
means but does not change the gate verdict. Full per-factory values at 0/64/128
are retained in the numerical report; the four physical witnesses remain in
selected endpoint evaluation traces.

## Known labels versus autonomous completion

The separate diagnostic executed 2,160 branches: three shared initial policies
and fifteen final policies, each tested from five anchors at three stages with
eight fixed sampling streams. Each table entry is a percentage attaining the
original demonstrated area, with forty branches per stage and learner seed.

| Arm | Pre-pose (%) | Pre-repair (%) | Pre-commit (%) |
| --- | --- | --- | --- |
| C | 0.0 / 0.0 / 0.0 | 0.0 / 0.0 / 0.0 | 0.0 / 0.0 / 2.5 |
| U05 | 0.0 / 0.0 / 0.0 | 0.0 / 2.5 / 0.0 | 0.0 / 5.0 / 2.5 |
| B05 | 7.5 / 2.5 / 10.0 | 40.0 / 27.5 / 55.0 | 47.5 / 60.0 / 62.5 |
| U50 | 0.0 / 0.0 / 0.0 | 7.5 / 5.0 / 5.0 | 15.0 / 15.0 / 17.5 |
| B50 | 17.5 / 12.5 / 17.5 | 52.5 / 47.5 / 52.5 | 60.0 / 60.0 / 57.5 |

All initial target-completion rates are zero. B50 completes from pre-repair in
47.5–52.5% of branches and from pre-commit in 57.5–60.0%, but from pre-pose in
only 12.5–17.5%. B05 also learns appreciable later-stage completion. Raising the
coefficient with uniform sampling (U50) improves later-stage behavior modestly
without producing a pre-pose completion. Thus successful later-stage imitation
does not establish reliable selection and completion of a useful edit.

The exact recorded-label probabilities tell a compatible, partial-learning story.
The following values are percentages, averaged over the five training labels at
each stage. These are known training anchors, not generalization measurements.
Full per-label log probabilities, ranks, kind/conditional probabilities, values
and positive weights at every fourth update remain in the retained anchor files.

| Seed | Policy | Pose label (%) | Repair label (%) | Commit label (%) |
| --- | --- | --- | --- | --- |
| 17 | initial | 0.164 | 1.241 | 0.689 |
| 17 | C | 0.229 | 0.132 | 1.219 |
| 17 | U05 | 0.203 | 1.062 | 1.349 |
| 17 | B05 | 1.015 | 36.060 | 45.549 |
| 17 | U50 | 0.222 | 15.875 | 19.977 |
| 17 | B50 | 2.778 | 81.019 | 50.602 |
| 29 | initial | 0.163 | 0.033 | 0.345 |
| 29 | C | 0.159 | 0.042 | 0.229 |
| 29 | U05 | 0.149 | 2.912 | 3.260 |
| 29 | B05 | 0.928 | 29.573 | 41.519 |
| 29 | U50 | 0.282 | 14.192 | 16.842 |
| 29 | B50 | 1.872 | 67.594 | 54.487 |
| 41 | initial | 0.479 | 0.185 | 3.177 |
| 41 | C | 0.227 | 0.226 | 0.675 |
| 41 | U05 | 0.210 | 1.237 | 2.071 |
| 41 | B05 | 1.381 | 47.636 | 53.044 |
| 41 | U50 | 0.272 | 11.091 | 20.877 |
| 41 | B50 | 4.003 | 81.161 | 46.758 |

No continuation branch hit its separate two-second wall guard. All 2,160 retain
the native 64-action horizon. Pre-pose starts retain attempts 3/26/35/37/54,
leaving different original action budgets. Reconstructed elapsed time differs
from the recorded value only by measured reconstruction overhead (at most
0.613 ms); the native allowance remains 300,000 ms. No clocks were reset and no
scripted repair or commit was inserted.

Of B50's 120 pre-pose branches, 19 improve and attain the target. All 101 failures
reach original attempt 64; 97 retain pending connections and 96 retain facility
conflicts. Many branches commit a valid but unimproved state: B50 has 74 valid
commit branches but only 19 useful pre-pose completions. A valid commit alone
must not be counted as successful optimization. These observations point toward
selection and multi-action draft control as the next diagnostic subject. They do
not isolate a unique cause or prove that more horizon or model capacity is needed.

## Realized replay exposure and discovery

Actual draws include PPO's actor-active epochs only; a KL stop also stops
imitation. Balanced arms maintain exactly 50% large draws. Uniform arms draw
large examples only about 1.2–2.2% of the time. The imported count below excludes
subsequently discovered child examples. Positive/zero counts sum the three
imported stage labels; they are uses, not independent examples or training runs.

| Seed | Arm | Large draws (%) | Imported draws | Imported positive/zero labels | Mean actor epochs | Child large entries eligible at 128 |
| --- | --- | --- | --- | --- | --- | --- |
| 17 | C | 0.00 | 0 | 0/0 | 3.87 | 0 |
| 29 | C | 0.00 | 0 | 0/0 | 3.49 | 0 |
| 41 | C | 0.00 | 0 | 0/0 | 3.84 | 0 |
| 17 | U05 | 1.50 | 233 | 683/16 | 3.79 | 0 |
| 29 | U05 | 2.24 | 334 | 991/11 | 3.63 | 0 |
| 41 | U05 | 1.20 | 190 | 552/18 | 3.86 | 0 |
| 17 | B05 | 50.00 | 7568 | 21814/890 | 3.70 | 0 |
| 29 | B05 | 50.00 | 5685 | 16576/479 | 3.05 | 1 |
| 41 | B05 | 50.00 | 7231 | 21182/511 | 3.77 | 2 |
| 17 | U50 | 1.59 | 230 | 644/46 | 3.54 | 0 |
| 29 | U50 | 2.02 | 252 | 739/17 | 3.04 | 0 |
| 41 | U50 | 1.35 | 203 | 568/41 | 3.68 | 0 |
| 17 | B50 | 50.00 | 4057 | 12009/162 | 3.16 | 13 |
| 29 | B50 | 50.00 | 4851 | 14503/50 | 2.69 | 3 |
| 41 | B50 | 50.00 | 4918 | 14333/421 | 3.29 | 3 |

Imported pose weights are positive in roughly 92.6–99.5% of their realized uses;
the imported labels usually contribute a positive loss, which is still
insufficient for reliable completion in this screen.
Balanced exposure greatly increases known-label uses. B50's imported share can
subsequently fall as its own large successes enter the fixed large half.
B50 has 13/3/3 child large entries eligible for the final update, versus 0/1/2
for B05 and zero for C/U05/U50. These are deduplicated replay entries, not counts
of independent factories. Eligibility is measured before the final optimizer
update; discoveries admitted by that update's terminal ingestion cannot
influence the endpoint and are not included in this eligibility count.

Actor-active epochs fall with the stronger imitation regime, especially for
seed 29. This is part of the realized treatment under the unchanged KL rule,
not a reason to equalize learning afterward. The numerical report retains
origin/stratum/stage positive counts and weight sums, origin draw totals, all
actor-epoch histograms, on-policy area reward and raw advantage scale. Subsequent
on-policy discoveries differ by arm, so these contrasts include discovery
feedback rather than representing fixed-data single-gradient causal effects.

## Every registered paired contrast

All differences are percentage points. The interaction is
(B50 - U50) - (B05 - U05). The primary column uses the same fifteen eligible
factories for all arms; late columns use the historical 100–128 window.

| Seed | Contrast | Strict transfer | All-16 transfer | Late small | Late large | Late all |
| --- | --- | --- | --- | --- | --- | --- |
| 17 | U05-C | +0.0000 | +0.0000 | -0.5641 | +0.0000 | -0.2644 |
| 17 | B05-U05 | +0.0000 | +0.0000 | -0.2538 | +0.0306 | -0.1027 |
| 17 | B50-U50 | +0.1042 | +0.0977 | -0.5145 | +0.1021 | -0.1869 |
| 17 | U50-U05 | +0.0000 | +0.0000 | +0.3347 | +0.0000 | +0.1569 |
| 17 | B50-B05 | +0.1042 | +0.0977 | +0.0740 | +0.0715 | +0.0727 |
| 17 | interaction | +0.1042 | +0.0977 | -0.2607 | +0.0715 | -0.0842 |
| 29 | U05-C | +0.0000 | +0.0000 | -0.5283 | +0.0000 | -0.2476 |
| 29 | B05-U05 | +0.0000 | +0.0000 | +0.9898 | +0.0000 | +0.4640 |
| 29 | B50-U50 | +0.0000 | +0.0000 | +1.3986 | +0.0087 | +0.6602 |
| 29 | U50-U05 | +0.0000 | +0.0000 | +0.5759 | +0.0000 | +0.2700 |
| 29 | B50-B05 | +0.0000 | +0.0000 | +0.9847 | +0.0087 | +0.4662 |
| 29 | interaction | +0.0000 | +0.0000 | +0.4088 | +0.0087 | +0.1962 |
| 41 | U05-C | +0.0000 | +0.0000 | -0.3461 | +0.0000 | -0.1622 |
| 41 | B05-U05 | +0.0000 | +0.0000 | -0.2231 | +0.0000 | -0.1046 |
| 41 | B50-U50 | +0.1958 | +0.1836 | +0.3685 | +0.0463 | +0.1973 |
| 41 | U50-U05 | +0.0000 | +0.0000 | -0.1693 | +0.0000 | -0.0794 |
| 41 | B50-B05 | +0.1958 | +0.1836 | +0.4223 | +0.0463 | +0.2225 |
| 41 | interaction | +0.1958 | +0.1836 | +0.5916 | +0.0463 | +0.3019 |

Supplying examples alone (U05 - C) does not improve final transfer or pre-pose
completion and worsens pooled small continuity. Balancing at 0.05 improves
known-state completion, but not final transfer. Increasing the coefficient under
uniform sampling also leaves final transfer at zero. Only their combination
shows any final transfer; it is too sparse to pass the declared practical gate.
The positive transfer interaction in two seeds is a descriptive screening
result, not a statistical significance or population-level effect estimate.

## Execution and retention

Main training took 6,468.6 seconds (107.8 minutes); the separately invoked
continuation diagnostic took 66.1 seconds. Every arm stayed within its one-hour
limit, taking 414.6–469.0 seconds. All 17,280 scheduled historical/transfer
evaluation episodes completed 64 actions; none exhausted a wall guard. Main
updates recorded eight-worker step concurrency throughout, with CUDA batched
learning and a maximum measured process-tree RSS of 13,549,076,480 bytes
(12.62 GiB), below 32 GiB.

Measured main phase totals were 2,235.6 seconds collection, 508.7 learner,
1,982.3 checkpoint serialization/storage, and 1,154.7 evaluation. These lightweight
phase counters omit startup, fixed-anchor inference and some orchestration.
The 108.9-minute training-plus-probe total exceeded the readiness forecast of
91 minutes. Main durable storage is a substantial observed cost; the small
readiness workload's timing should not be treated as an exact full-run forecast.
The verified 2.296x readiness speedup and common eight-worker/CUDA contract
remain documented separately; this screen did not re-run a serial counterfactual.

Checkpoints 0/64/128, latest recovery states, complete active-buffer source
episodes, actual input payloads, both panels, all compact evaluation/anchor
measurements and selected physical traces are retained. Normal evaluation traces
outside 0/64/128 and obsolete recovery checkpoints were pruned during execution.
The five imported parent bundles remain intact. No raw run output is committed.

The explicit `run`, then `probe`, then `report` CLI commands all completed.
The report verified all fifteen endpoints, 512 actual inputs, full action/episode
counts, eight late evaluations per arm, historical strata and forty continuation
branches per stage. The runtime's source verification and native/optimizer tests
passed before launch; see the runtime report. This results slice changes only
documentation and compact provenance and passes the repository worktree/index
artifact checks.

## Bounded conclusion

The policy can learn portions of the supplied large edits when they receive
substantial replay exposure. B50 also discovers additional large successes and
produces a few validated improvements on eligible new geometries. The remaining
gap is substantial: useful pre-pose completion remains infrequent and transfer
is limited to two distinct food factories across the three learner seeds.

Do not promote a coefficient, extend training, enlarge the model or relax action
budgets from this screen. A next bounded design should distinguish selecting a
useful edit from preserving and finishing it, using the retained pre-pose traces
and explicit pending/conflict/commit transitions. The current comparison does
not rule out other coefficients, replay objectives or discovery methods.
