# Post-hoc cold-study regression diagnosis

Date: 2026-09-14. Status: completed observational diagnosis; no learning change or
new training/evaluation rollout. The main run remains paused at update 1908.

## Finding and research decision

The retained evaluations do not establish a common late-training collapse.
Seed 113 declined from update 96 to 128, but the three methods lost on different
factories and changed their actions differently. Seed 71's development panel
improved over the same interval for all three methods. Training seed and panel
both differ in that comparison, so it cannot isolate the source of variation.

A separate, concrete efficiency problem is visible in the final traces:
**40.80–55.74% of all decisions attempt bands that the native engine must reject
because they cut a rigid body/component or remove an original anchor.** These
decisions cannot improve or prepare the draft. This is an observed opportunity,
not an established explanation for the late decline: rejection actually
decreased in baseline and uniform-16 while their scores declined.

Keep baseline as the reference and stop expanding the replay-weight search on
this evidence. The next narrow candidate is a frozen-model A/B evaluation that
excludes only these structurally impossible band proposals, before spending on
retraining. Preserve overlapping, temporarily worse, and routing-pending edits;
those are required preparation actions. The acceptance question is whether
freed decisions improve retained validated area, including large factories,
within the same action/time budgets and acceptable candidate-preparation cost.
The effect is unmeasured; this report does not adopt a mask or select a model.

## Evidence and interpretation limits

The diagnosis reads 30 existing reports: three methods, two seed/panel pairs,
and updates 0/32/64/96/128. Each report has 32 geometries (16 small, 16 large),
two fixed action seeds each, and 64 charged decisions. Existing study validation
checks physical area, geometry/product/input identities, action replicates, and
validated retained outputs. There are no resource-terminated episodes in these
reports. The score is percent reduction of the original factory's used area;
changes below are percentage points (pp), averaged first within a geometry.

For temporal comparisons, the exact panel, original inputs, action seeds, and
budget remain fixed. A fixed action RNG does not force the same trajectory
under changed policy probabilities. Only two action replicates are available;
checkpoint selection and stochastic evaluation remain material uncertainty.
The 96→128 interval follows inspection of the curves; 64→128 is a sensitivity
comparison, not a pre-registered independent test.

Final update-128 raw traces survive for all 192 confirmation episodes. Earlier
raw episode snapshots were removed by normal retention; their reports retain
scores, milestones, action counts, pending/overlap counts, and output validity.
Consequently the exact late-checkpoint trajectories can be inspected, but exact
earlier action sequences cannot be compared without a new evaluation. No native
replay, model inference, or checkpoint restoration was run for this diagnosis.

The paired intervals below resample whole geometries within size strata using
the existing study bootstrap (10,000 draws, RNG seed 714600000). They condition
on one training seed and its two observed action replicates. They do not include
action-sampling uncertainty, checkpoint-selection bias, or training-seed
variation, and are not evidence of a causal training failure.

## Where the decrease occurs

| Seed/panel | Method | Update 96 | Update 128 | Change pp | Geometry win/tie/loss | Conditional 95% interval pp |
| --- | --- | ---: | ---: | ---: | --- | --- |
| 113 / confirmation | baseline-8 | 23.638% | 20.287% | -3.351 | 5 / 10 / 17 | [-5.726, -0.941] |
| 113 / confirmation | priority-8 | 24.901% | 22.713% | -2.188 | 10 / 5 / 17 | [-5.016, 0.888] |
| 113 / confirmation | uniform-16 | 24.971% | 23.529% | -1.442 | 10 / 8 / 14 | [-4.627, 1.218] |
| 71 / development | baseline-8 | 31.936% | 33.003% | +1.068 | 17 / 4 / 11 | [-1.635, 3.608] |
| 71 / development | priority-8 | 30.932% | 33.934% | +3.002 | 18 / 6 / 8 | [1.250, 4.897] |
| 71 / development | uniform-16 | 31.741% | 32.366% | +0.625 | 12 / 10 / 10 | [-1.383, 2.666] |

From 64→128, confirmation changes are -2.698 / -1.465 / -2.074 pp in the
same method order; all three conditional intervals include zero. Development
changes are +0.943 / +4.435 / -0.525 pp. Thus the visual impression depends on
the interval as well as the training seed and panel.

The confirmation losses are neither one universally broken factory nor a
uniform shift across all factories:

| Method | Gross loss pp | Offset by gains pp | Top five share of gross loss | Both action replicates decline | Small / large change pp |
| --- | ---: | ---: | ---: | ---: | --- |
| baseline-8 | 4.446 | 1.094 | 51.5% | 6 / 32 | -4.658 / -2.045 |
| priority-8 | 4.461 | 2.273 | 55.2% | 4 / 32 | -4.980 / +0.604 |
| uniform-16 | 3.196 | 1.755 | 70.8% | 5 / 32 | -1.149 / -1.734 |

Gross loss sums negative geometry changes and divides by all 32 geometries;
gross gain uses positive changes with the same denominator. Their difference
is the net decline. Only generation seeds 719200040, 719200048, 719200264, and
719200776 decline in all three methods. Product changes also differ:

| Method | Watered grass | Food | Medical | Copper liquid |
| --- | ---: | ---: | ---: | ---: |
| baseline-8 | -1.036 | -0.986 | -3.406 | -7.475 |
| priority-8 | -7.315 | +0.736 | +0.435 | -3.164 |
| uniform-16 | -7.007 | -0.941 | -2.755 | +3.407 |

## What the actions show

All values in this table are mean counts per 64-decision confirmation episode.
Pending and overlap counts describe the state after an action, including a
rejected action; they do not count independent faults caused by that action.

| Method | Checkpoint | Place | Insert band | Delete band | Rejected | Pending state | Overlap state |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| baseline-8 | 96 | 20.02 | 0.73 | 43.25 | 31.72 | 28.05 | 15.34 |
| baseline-8 | 128 | 24.33 | 0.53 | 39.14 | 29.05 | 31.56 | 17.30 |
| priority-8 | 96 | 14.91 | 2.30 | 46.80 | 35.28 | 30.23 | 13.38 |
| priority-8 | 128 | 12.05 | 5.75 | 46.20 | 36.25 | 29.06 | 13.72 |
| uniform-16 | 96 | 17.25 | 2.31 | 44.44 | 33.55 | 27.50 | 16.41 |
| uniform-16 | 128 | 25.16 | 2.36 | 36.48 | 26.72 | 34.25 | 26.94 |

Baseline and uniform shift toward placement, while priority shifts toward
insertion. Uniform finishes with an unfinished draft in 55/64 episodes, up from
50/64; baseline stays 47/64 and priority stays 45/64. Every returned best still
passes native validation. Draft incompleteness is allowed and does not by
itself prove defective learning or justify suppressing preparatory moves.

The largest uniform decline is factory **719200304** (watered grass):
its two-replicate mean falls from 40.952% to 4.048%, a -36.905 pp geometry
change that contributes -1.153 pp to the overall -1.442 pp net decline. That
is 36.1% of gross losses; comparing to net decline alone exaggerates its share
because gains on other factories offset losses.

At update 128, its samples use 49 and 53 placement actions. There are 46 and 55
post-action pending states, and 42 and 52 overlap states. Area improves just
once per sample, at actions 10 and 6, then never again within the budget.
The saved areas are 208 and 195 from an initial 210; update-96 reports instead
record saved areas 144 and 104. The traces establish late-episode failure to
convert continued editing into a better valid factory. They do not establish
which policy update caused that behavior or that it would persist under more
action seeds.

The largest baseline loss (719200808) and priority loss (719200816) also have
late traces with long stretches after their last area improvement. Their
last improvements occur at actions 22/2 and 23/12, respectively. Their pending,
overlap, and rejection patterns vary. There is no single demonstrated late
failure mechanism shared by all three methods.

Across all final traces, structurally impossible bands are a clearer common
source of consumed decisions:

| Method | Cuts facility body | Cuts original component | Removes original anchor | Total / 4096 actions |
| --- | ---: | ---: | ---: | --- |
| baseline-8 | 1532 | 196 | 68 | 1796 (43.85%) |
| priority-8 | 1970 | 216 | 97 | 2283 (55.74%) |
| uniform-16 | 1428 | 154 | 89 | 1671 (40.80%) |

The input encoder currently enumerates band positions and configured widths
without these geometry checks. Native `declarative/edit.rs::band` rejects a
band intersecting a rigid body/component or deleting an original anchor.
The accepted edit contract explicitly says syntactic rejection changes neither
draft nor best. These three diagnostic counts therefore describe guaranteed
non-progress decisions, unlike pending routing or temporary overlap.
Removing them would change the policy's candidate domain and per-kind
normalization; it is an explicit experiment, not a semantics-free optimizer
replacement or a silent reinterpretation of old replay labels.

## Reproduction and retained evidence

The concise findings and bindings are in
`2026-09-14-cold-quality-regression-diagnosis.manifest.json`. Complete derived
analysis and condensed final action records remain under ignored
`artifacts/cold-quality-regression-20260914/`. Original frozen evidence remains
on `ashrimp-arch` under
`/home/ashrimp/Srv/aic-quality-cold-continue-20260914/artifacts/study`.
The completed cold study and unchanged adoption decision are documented in
`2026-09-14-cold-start-quality-replay-study.md`.

With the study tree locally accessible, collect existing reports and inspect
final snapshots explicitly (stdout files belong under ignored artifacts):

```sh
tools/factory-edit-pilot/.venv/bin/python tools/factory-edit-pilot/operations/cold_quality_regression.py collect --study-root /path/to/frozen/artifacts/study > artifacts/cold-quality-regression-20260914/evaluation-bundle.json
tools/factory-edit-pilot/.venv/bin/python tools/factory-edit-pilot/operations/cold_quality_regression.py traces --study-root /path/to/frozen/artifacts/study > artifacts/cold-quality-regression-20260914/traces.json
tools/factory-edit-pilot/.venv/bin/python tools/factory-edit-pilot/operations/cold_quality_regression.py analyze --bundle artifacts/cold-quality-regression-20260914/evaluation-bundle.json --evidence artifacts/cold-quality-continuation-20260914/final-evidence.json > artifacts/cold-quality-regression-20260914/analysis.json
```

The utility reuses the existing study aggregation/bootstrap and optimized
one-pass snapshot inflation. It adds no normal-run validation, saving, worker,
or timing overhead. Here its trace command was streamed over SSH to the frozen
checkout's existing Python runtime; no file was installed in that checkout.
Report hashes are captured at collection; identity/area checks run at the
analysis boundary. Selected snapshot hashes and aggregate diagnostic counts
are retained in the compact manifest. The manifest records the original local
evidence hashes; re-collecting JSON with different whitespace can change a
bundle hash without changing its bound source reports.

Verification: the analysis CLI validated all 1,920 report rows; the trace CLI
checked all 192 retained final episodes against their report start/best scores,
64 charged actions, and native-validity flag. Its action-kind/status aggregates
also match every corresponding report. The collection CLI reproduces the
original 30-report bundle semantically. `cargo check --workspace`,
`cargo test --workspace`, and the worktree/index tracked-file checks passed.
No learner or native code changed.
