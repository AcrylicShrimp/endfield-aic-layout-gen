# 256-update checkpoint in the ongoing cumulative study

Status: verified 256-update report; the later study was stopped by the user before 1024.
Date: 2026-09-07. Allocation and interpretation: `budget-curve-contract.md`.
The continuation decision below is historical. See the
[paused handoff](../../handoffs/2026-09-07-layout-learning/README.md) before resuming work.

## Decision at this checkpoint (historical)

Continuing from 64 to 256 updates has not demonstrated a consistent additional benefit in
this development population. The mean paired win-minus-loss endpoint against the initial
policies remains positive, but falls from 0.18056 to 0.09722. Directly comparing 256 against
64 gives 34 wins, 36 losses and two ties, a mean endpoint of -0.02778. This is approximately
flat finite development evidence, not proof of equal population performance or a reason to
stop the declared experiment. Individual outcomes and score components are nonmonotonic.

All six prospective target evaluations still return `[2132,392,147]`, 41 by 52, with
29.6904% empty XY area. Development futile actions average 79.5844%; target futile actions
average 84.6842%. Production readiness, low waste and the target's 1% criterion remain unmet.
Continue the same learners to 1024 without changing reward, model, horizon or admission.
This checkpoint neither rules out insufficient training nor proves global impossibility.

## Matched development outcomes

Each seed uses twelve frozen complete validation factories, two sampling seeds and 1024
proposals per episode. Wins compare area, physical transport tiles and turns lexicographically.
The initial policy is an active 1024-proposal control, not a do-nothing layout.

| Training seed | Against initial: wins / losses / ties | Against uniform | Against 64 updates | Initial-control endpoint, 64 -> 256 |
| --- | --- | --- | --- | ---: |
| 1901 | 13 / 11 / 0 | 16 / 8 / 0 | 11 / 13 / 0 | 0.16667 -> 0.08333 |
| 2901 | 12 / 9 / 3 | 13 / 7 / 4 | 11 / 11 / 2 | 0.16667 -> 0.12500 |
| 3901 | 13 / 11 / 0 | 13 / 11 / 0 | 12 / 12 / 0 | 0.20833 -> 0.08333 |

The 256-update mean endpoint is 0.09722, seed range 0.08333-0.12500. Combined descriptive
counts are 38/31/3 against initial and 42/26/4 against uniform. The independent learning unit
is the training seed, not the 72 correlated development comparisons. Three seeds and twelve
small validation roots do not establish arbitrary-factory generalization or precise intervals.

| Seed | Mean area / tiles / turns difference from initial | Difference from 64 updates |
| --- | ---: | ---: |
| 1901 | -14.8333 / -13.5833 / -0.8333 | +9.8750 / -2.1250 / +0.4167 |
| 2901 | -7.3750 / +4.1667 / +0.9167 | -8.6250 / -2.2917 / +0.7083 |
| 3901 | -15.1250 / -9.1250 / -0.2500 | +4.3750 / 0.0000 / +0.5833 |

These component means are descriptive, not a replacement weighted objective. All per-root
retained tuples, traces, work counts and witnesses remain in the referenced evaluation files.

## Futility and consumed work

Futile means rejected, unchanged or an exact-state revisit; valid temporary deterioration is
not automatically futile. Proposal counts and route bounds match; consumed work need not.

| Population / seed | Futile %, initial / 64 / 256 | Route expansions, initial / 64 / 256 |
| --- | ---: | ---: |
| Development / 1901 | 79.3579 / 79.1789 / 79.5898 | 870,198 / 665,541 / 718,128 |
| Development / 2901 | 78.9185 / 79.6956 / 79.5817 | 783,234 / 757,097 / 813,517 |
| Development / 3901 | 79.1056 / 79.2318 / 79.5817 | 786,196 / 721,980 / 795,104 |
| Target / 1901 | 84.8633 / 83.4961 / 83.6426 | 35,643 / 48,643 / 44,553 |
| Target / 2901 | 85.3516 / 85.4004 / 85.7910 | 49,943 / 52,471 / 43,921 |
| Target / 3901 | 84.6191 / 84.9609 / 84.6191 | 44,451 / 45,763 / 46,087 |

Development futility increases by 0.4571 percentage points against initial and 0.2157 points
against 64. Development route expansions increase from 64 in every seed. Target futility
changes by -0.2604 points against initial and +0.0651 against 64, without any target quality
gain. Neither a smaller waste fraction nor fewer route expansions proves better packing.

| 256-update population / seed | Summed inference seconds | Summed episode seconds |
| --- | ---: | ---: |
| Development / 1901 | 38.610 | 283.107 |
| Development / 2901 | 39.691 | 290.465 |
| Development / 3901 | 40.591 | 294.322 |
| Target / 1901 | 10.133 | 44.429 |
| Target / 2901 | 10.053 | 43.646 |
| Target / 3901 | 9.567 | 42.925 |

These are sums of per-episode times from co-running jobs, not isolated throughput benchmarks
or command wall time. Earlier checkpoint timings and route milliseconds remain in the raw
evaluation records; concurrent load prevents a causal speedup claim.

## Actual construction exposure through 256

Each prefix includes 224 freshly generated complete factories and 32 declared full-target
replay groups. Generation ledgers are cut at the 224th eligible root and checked against the
actual ordered training roots, rather than including later live generation.

| Seed | Attempts | Accepted / single-ineligible | Fresh medium | Largest fresh | Training target families |
| --- | ---: | ---: | ---: | ---: | ---: |
| 1901 | 1,660 | 341 / 117 | 16 | 52 | 94 |
| 2901 | 1,946 | 361 / 137 | 6 | 52 | 96 |
| 3901 | 1,487 | 341 / 117 | 11 | 44 | 92 |

The 5,093 attempts contain 1,043 accepted complete factories, 2,822 flow rejections, 856
construction stops, 346 duplicates, 25 construction-budget stops and one instance-guard stop.
Of the accepted factories, 371 single-facility roots are learning-ineligible and 672 enter
training. There are no observation-support exclusions. The 33 trained medium roots all have
one internal weak component. No fresh large root enters training; eighteen solved large
requests appear among generation attempts. No failed request was cropped or replaced by a
saved easier root.

The prefixes cover 135 training target families and 592 distinct production fingerprints
across 672 fresh groups. Cross-seed repeats are not independent new problems, and these
fingerprints are not graph-isomorphism certificates. The validation population is still
2-14 facilities plus the explicitly trained target: medium training exposure is not held-out
medium/large generalization. Parallel-component admission remains separately unapproved.

## Direct geometric inspection

The same preregistered first training/sampling seeds select the target and the largest
14-facility validation root. The view contains their 64- and 256-update comparisons against
initial, using one shared tile scale per root across all displayed checkpoints. All eight
displayed frames pass fresh geometry and original full-request coverage validation; exact
scores and densities reproduce from the actual witnesses.

- The 256-update target visibly coincides with its initial control: facility rows, central
  voids and the right-side pipe corridors remain. There is no hidden packing improvement.
- The fixed 14-facility case changes from `[1400,518,91]` initially to `[1295,453,82]` at 64,
  then `[1435,439,76]` at 256. The last layout has fewer transport tiles and turns but a
  larger footprint than both controls, so it is worse under the configured score order.
  F03 moves down and F06 up compared with initial; the long left-side belt loop is reduced.
  At 256, the right-hand facilities and connections extend the used width to 41, a large upper-left
  void remains, and the central parallel corridors and lower-right empty region are visible.
  Empty fraction rises from 49.7143% initially to 56.4460%; the largest empty rectangle is
  9 by 9. This is not dense packing. Obstacle-free chain excess falls from 54 to eight, but
  that measure does not prove that remaining wiring is redundant or nonredundant.

The main agent inspected the target and 14-facility 256 pairs, the 64-update 14-facility pair,
and the 256-update 14-facility pair in dark theme. Twenty-four browser cases pass across
light/dark and 360/736/1024 widths, including selection, readable labels and matching scales.
The visualization skill guided common-scale actual geometry and theme/accessibility checks;
the drawings are not generated illustrations or substituted factories.

View: `target/layout-direct-budget-visual-256-01/cumulative-layouts.html`, 201,162 bytes.
The final post-hoc regression case remains deferred until all 1024-update runs finish.

## Evidence and continuation

All 3,072 training episodes through 256 and all 312 initial/uniform/64/256 evaluation episodes
were independently hash-checked and audited against exact effort and semantic summaries.
Training-root and full-capsule hashes match, and initial/64/256 snapshots reproduce the
persisted model/Adam/RNG continuity chain. The boundary accounts for 1,572,864 training and
319,488 evaluation proposals, 1,892,352 total; it is not the increasing live count.
Each seed's updates 1, 64 and 256 pass the runner's exact update-replay check. Intermediate
non-capsule updates are not claimed to be independently replayable from saved observations.
All frozen source hashes match. No training source, configuration or catalog was changed.
All three seeds have continued past evaluation with update 257's before-state hash equal to
update 256's after-state hash. Nineteen direct Node tests, thirty-five Python tests,
`cargo check --workspace` and `cargo test --workspace` pass; two existing Rust tests remain
ignored. No Rust edits, dependency changes or production-code cutover were made.

| Seed | 256-update evaluation SHA-256 |
| --- | --- |
| 1901 | `343a9a1f3463991c08e9dfef2d8c8ba32523da72bc7f675f334c78150b0c15d5` |
| 2901 | `6fa9e6c296c0381d1acbbc4d21733df82da9fc1f8d9594a0d065d41950269a85` |
| 3901 | `b41d5091c7a2101c25c115267b7103e8626b2006f313f59e50e2e3a542c18474` |

The files are `evaluation-0256-trained.json` in the seed directories from
`budget-curve-progress.md`. Reproduce the view with the existing `budget-curve-visual.mjs`
command using `--checkpoint 256` and a new exclusive output directory; never restart training
to reproduce it. This measurement-only slice preserves the accepted constructive design:
connected immediate routing, immutable complete valid best, area/tiles/turns priority,
unchanged external obligations and no exact-solver fallback. No production API was changed.

Next: finish the preregistered 1024 allocation, analyze all three fixed final snapshots and
inspect the required final/worst-case geometry. Do not select the more favorable 64 checkpoint
as the final budget result, or reinterpret this finite plateau as an impossibility proof.
