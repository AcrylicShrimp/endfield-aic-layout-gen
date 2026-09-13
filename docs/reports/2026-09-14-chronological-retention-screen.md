# Chronological retention and sampling screen

> Withdrawn from scientific selection on 2026-09-14 at the user’s request.
> Historical execution evidence only; do not use these results or recommendations
> to select a training setup. Replaced by the [quality-focused study](../designs/2026-09-14-quality-weighted-replay-study.md).

The preregistered screen completed all **25 profiles × three seeds** and returned
**`no_candidate`**. Retain the current baseline as the supported operational
choice. This is not evidence that baseline learning is superior: the frozen
experiment cannot establish learning improvement, and its small-stratum coverage
ceiling makes the registered five-percentage-point adoption gate unattainable.

## Experiment and common cohort

The complete baseline arrival corpus covers updates 1745–1848 from the immutable,
paused update-1744 parent. It contains 14,729 distinct new semantic IDs. The common
first-half cohort contains 7,402 IDs with 32 complete subsequent updates available.
Facility-edit cohorts contain 16 IDs from 16 large-factory episodes and 92 IDs
from 77 small-factory episodes. Both exceed the registered minimum of ten episodes.
IDs absent from an arm remain in its denominator; re-admission never resets the
anchor or follow-up horizon. Initial entries are reported separately as
left-truncated, and admission spells retain their right-censoring status.

Each arm starts with the same 4,096 entries and original utility. The parent
model computes one stage-balanced frozen raw SIL contribution and positive
advantage for each of 18,825 unique sequences. Each complete compact source is
verified and decoded once, with bounded packed inference and no optimizer.
Every arm uses the production sampler, its own sampled utility observations,
priority refresh/bootstrap when applicable, and the shared replacement index.
The actual KL-dependent minibatch-call schedule is replayed before ordered
terminal admissions; new admissions cannot be sampled in the same update.

The matrix uses counts 8/16/32, v1 or v2 category fraction 0.10 with 64-update
grace, uniform/rare-3pp mixture/positive-advantage sampling, and a q=0 area-order
control. The [native audit](2026-09-14-success-native-geometry-audit.md) permitted
six optional geometry-package profiles. Seeds are 17, 29 and 41. Capacity remains
4,096 sequences / 262,144 labels in every arm.

## Findings

Values below are medians across three paired seeds. Exposure means at least one
draw during the common 32-update horizon. Positive exposure requires positive
raw frozen SIL contribution. Neither means a measured trained-policy gain.

| Retention / sampling / count | Exposure large / small | Positive exposure large / small | Mean draws large / small | Retained updates large / small |
| --- | --- | --- | --- | --- |
| v1 / uniform / 8, baseline | 93.75% / 97.83% | 75.00% / 82.61% | 3.12 / 3.73 | 24.25 / 31.30 |
| v1 / uniform / 16 | 100% / 100% | 81.25% / 84.78% | 5.62 / 6.55 | 24.00 / 28.77 |
| v1 / uniform / 32 | 100% / 100% | 81.25% / 84.78% | 9.69 / 11.65 | 20.81 / 24.24 |
| v1 / rare-3pp / 8 | 100% / 97.83% | 81.25% / 82.61% | 3.94 / 4.12 | 24.25 / 31.34 |
| v1 / positive advantage / 8 | 100% / 100% | 81.25% / 84.78% | 2.81 / 4.72 | 24.00 / 28.83 |
| v2 / uniform / 8 | 100% / 98.91% | 81.25% / 83.70% | 4.00 / 3.87 | 32.00 / 32.00 |
| v2 / rare-3pp / 8 | 100% / 98.91% | 81.25% / 83.70% | 4.94 / 3.97 | 32.00 / 32.00 |
| v2 / positive advantage / 32 | 100% / 100% | 81.25% / 84.78% | 56.50 / 79.39 | 32.00 / 32.00 |

Baseline exposure ranges were 87.50–100% for large and 96.74–98.91% for small.
The small-stratum median of 97.83% can improve by at most 2.17 percentage points.
Every alternative therefore fails the required five-point improvement in both
strata. The criterion was not changed after seeing this ceiling.

Category protection does preserve the entire 32-update facility-edit horizon in
this corpus. More draws and priority sampling also change exposure materially.
Across all sampled replay entries, positive-contribution draw fraction rises
from a baseline median 59.47% to 85.43% with v1 positive-advantage/eight sampling.
These are useful descriptive signals, not reasons to override the preregistered
adoption gate. Priority importance correction affects reported effective loss,
while eviction utility and priority measurements retain their raw values.

All six geometry packages fail the additional matched two-point improvement
criterion. The v1 geometry/eight package also fails the large-stratum improvement
gate. Passing the local native loop audit did not establish usefulness in this
chronological comparison. The full receipt contains quantity-only, sampling-only,
retention-only, combined and matched geometry contrasts, per-seed ranges,
per-ID concentration, effective sample size, composition and eviction reasons.

Every seed/profile passed hard capacities and source validity. All alternatives
passed the ordinary-deletion geometry coverage and nondecreasing positive-exposure
gates. Baseline sampled ordinary-deletion geometry counts were 380–383 large and
279–280 small, so the relative coverage denominator was defined.

## Storage and runtime

Storage prices actual entry references against complete diagnostic union chunks,
combined with corpus-recorded live credit and normal retained-checkpoint roots at
each boundary. Every row reference must belong to its declared union coverage.
Inactive historical migration snapshots in the initial inventory are not normal
retained roots at these boundaries. Original baseline published bytes are reported
separately, as are active-only, mandatory-root and total common-representation
bytes. Candidate checkpoint storage is not simulated or claimed measured.

| Profile | Median peak common-root source bytes | Maximum paired ratio to baseline |
| --- | ---: | ---: |
| v1 / uniform / 8 | 748,461,391 | 1.000 |
| v1 / uniform / 16 | 892,026,594 | 1.194 |
| v1 / uniform / 32 | 1,178,674,656 | 1.580 |
| v1 / rare-3pp / 8 | 748,461,391 | 1.006 |
| v1 / positive advantage / 8 | 766,861,799 | 1.027 |
| v2 / uniform / 8 | 892,737,164 | 1.193 |
| v2 / positive advantage / 32 | 981,445,931 | 1.311 |

All eight 32-draw profiles exceed the 125% storage gate in at least one seed.
Retained segment count alone would not reveal this cost. Source-root estimates
are conservative common-representation comparisons, not actual child disk savings.

The entire successful screen took **293.616 seconds**, below 600 seconds.
One-pass inference took 90.613 seconds on one CPU thread, decoding 220 source
chunks. Peak observed RSS was 1,616,519,168 bytes, below the existing 48 GiB limit.
Added screen outputs total 338,940,794 bytes (339,152,896 allocated), below 2 GiB.
Sampling and replacement timing are separated per arm; baseline medians across
104 updates were 0.965 and 0.654 seconds respectively. These exclude optimizer,
normal collection and publication. The earlier diagnostic capture/write cost is
reported [separately](2026-09-14-success-arrival-capture.md); none of these figures
is presented as normal candidate-training throughput.

## Verification and reproducibility

Six focused screen tests passed with the release native library; the combined
retention/category/sampling/screen suite passed 30 tests without skips. Tests cover
common anchors across eviction/re-admission, same-update ordering, actual capacity,
paired deterministic draw/victim traces, missing mandatory source rows, frozen
packed imitation equivalence, zero coverage denominators and the fixed ceiling
rule. Existing indexed-oracle/default-sampler tests supply the shared algorithm
checks. Actual CLI help passed; an unregistered configuration was rejected with
exit 2 and a stable diagnostic. Workspace check passed. The first workspace test
attempt hit a CLI parser stack overflow under the default test stack;
`RUST_MIN_STACK=16777216 cargo test --workspace` passed. No Rust sources changed.
Worktree/index artifact hygiene passed before commit.

A first preflight rejected an inactive historical migration snapshot as if it
were a mandatory root. The validator was corrected to use the recorded normal
retained-checkpoint sets; required missing roots still reject. That attempt ended
in 0.339 seconds before inference or arm execution. No completed matrix was rerun,
and no threshold, model, profile, coverage requirement or budget was changed.

From `/home/ashrimp/Srv/aic-retention-candidates-20260914`, using the retained server
Python environment:

```sh
python tools/factory-edit-pilot/diagnostics/declarative_retention_screen.py compare \
  --corpus /home/ashrimp/Srv/aic-retention-experiments-20260914/artifacts/arrival-capture/corpus/manifest.json \
  --config tools/factory-edit-pilot/diagnostics/declarative-retention-screen.json \
  --native-audit artifacts/native-audit/report.json \
  --output artifacts/retention-screen-complete
```

The report SHA-256 is
`092a6858569d6c96b36dfc71cb30aefd404375ee48dcb0103a364fbff0dd151c`.
Registered configuration SHA-256 is
`6bbe1b1a3887fffc58b94ca251d072ae7dde9cf5e310b8b77b6794dd9e8cdfdf`.
Frozen parent model digest is
`68ea7ce2fd76c9a7e6c498d85eb0573ee23657cd1252ee22ced3c1e0acfa540a`.
The receipt binds all six numerical/screen source files, source corpus and native
eligibility report. Full arm rows, spells, boundaries and frozen cache remain in
the server output directory. The report is also retained locally at
`artifacts/retention-experiments/20260914/retention-screen-report.json`.

The actual arrival order and frozen model cannot predict candidate-generated
successes, changing critic estimates or candidate KL stops. Only three paired
sampling seeds and one parent are used; no significance or general optimality
claim is made. With no shortlist, the reviewed downstream transition and learning
experiments are not applicable. Their runtime gates remain unchecked. See the
[final setup verdict](2026-09-14-training-setup-verdict.md). Main training remains
paused at update 1744.
