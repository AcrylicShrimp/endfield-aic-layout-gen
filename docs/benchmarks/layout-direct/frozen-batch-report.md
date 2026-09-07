# Independent frozen-policy batch pilot

Status: complete declared allocation; no reliable learned-quality advantage established.
Date: 2026-09-07. Contract: `frozen-batch-contract.md`; config: `frozen-batch.json`.

## Decision

The current prefix estimator produces nonzero, partly aligned task gradients, but this
pilot does not establish that its one-step updates improve prospective factory layouts.
The small-root paired estimate is positive with a batch-bootstrap interval spanning zero.
On the 59-facility target, prefix and unchanged-policy outcomes tie on all 32 matched
evaluation pairs; no arm reduces target area. A lower observed gradient covariance on the
target does not itself imply better packing. Production readiness and the 1% empty-space
criterion remain unmet; neither model incapacity nor global impossibility is established.

Do not automatically switch to the user's hypothesized absolute-area/density reward, add
immediate penalties for valid detours, enlarge the model, or extend training on this evidence.
Next isolate whether the current representation/decoder can learn and execute known improving
choices on complete factory states, including contrasts with rejected choices. Keep this a
capacity/credit diagnostic, not a replacement product or an imitation-success release claim.
The separate medium/large construction-admission policy proposal is still unapproved.

## Population, controls and artifacts

Every root, batch and arm starts from the identical initial width-32 policy and empty Adam
snapshot, learning rate 0.003, norm-1 clipping, entropy zero. This is sixteen independent
single-update trials, not sixteen successive training updates. The seven-facility root is
the explicitly declared already-compacted full-factory descendant `[567,86,26]`; the other
root is the original complete 59-facility target. No new factories were generated, no
requests were cropped, and no held-out generalization population was sampled.

Eight four-trajectory behavior batches per root execute 128 proposals each: 8,192 behavior
actions. Four update arms each receive four separate matched evaluation seeds per batch,
also at 128 proposals: 32,768 evaluation actions. All 40,960 proposals execute; all 320
final episodes pass independent geometry and original full-factory coverage audits. Roots
receive additional fresh audits. None of the episodes terminates on its elapsed-time bound.

Run: `target/layout-direct-frozen-batch-01/report.json`, SHA-256
`f1d8099372fd8214da9341355a60766938534479247be33420c2da1ff93ada29`.
Elapsed time: 824.68 seconds, with other local checks overlapping; not an isolated throughput
benchmark. The complete evidence occupies approximately 958 MiB on disk, below the 2 GiB
guard. This remains expensive instrumentation, not a recommended production storage format.

The run preserves sixteen replayable prefix-update capsules and all 64 arm snapshots,
signal arrays, raw gradient vectors and prospective episode references. All recorded source
hashes still match. Reading the persisted files independently reproduces the aggregate
statistics, all 320 episode hashes and semantic summaries, 80 distinct behavior/evaluation
seed namespaces, and the full action/audit counts. Saved episode elapsed times are later
than the preceding summary call; they are checked as monotonic, not incorrectly required
to match exactly. No other summary field differs.

## Prospective paired quality

Wins/losses compare final retained tuples lexicographically: area, physical tiles, turns.
The endpoint is mean wins minus losses, with ties contributing zero. Intervals resample
the eight complete update batches, each containing four evaluation pairs; they do not
treat 4,096 action steps or 32 dependent-through-update episodes as independent updates.

| Root | Update arm | Wins / losses / ties | Mean paired endpoint | Batch-bootstrap 95% interval |
| --- | --- | --- | ---: | --- |
| Seven facilities | Prefix | 10 / 7 / 15 | 0.09375 | [-0.03125, 0.21875] |
| Seven facilities | Same-outcome terminal | 6 / 5 / 21 | 0.03125 | [-0.15625, 0.18750] |
| Seven facilities | Cyclic reassignment | 5 / 5 / 22 | 0 | [-0.21875, 0.21875] |
| 59 facilities | Prefix | 0 / 0 / 32 | 0 | [0, 0] |
| 59 facilities | Same-outcome terminal | 0 / 0 / 32 | 0 | [0, 0] |
| 59 facilities | Cyclic reassignment | 0 / 1 / 31 | -0.03125 | [-0.09375, 0] |

Zero versus itself is an exact tie in every case. A degenerate [0,0] bootstrap interval
on observed target ties is **not** proof of zero population effect or equivalence: the
resampling procedure cannot invent unobserved wins or losses. Eight batches are a pilot,
not adequate evidence of arbitrary-factory effectiveness.

Prefix-minus-zero mean component differences on the small root are `[-4.71875, +4.46875,
+1.3125]`: modest average area reduction with more transport and turns. This is not a
weighted objective. Both the prefix and zero policies reach area 399 on some episodes;
zero alone improves the starting area in 31/32 episodes. Much of the available improvement
therefore does not require the learned update. On the target, some episodes improve routes
to `[2132,386,145]`, but prefix and zero attain the same quality on every matched seed.
Do not misreport the target finding as no route improvement relative to the initial root.

| Root | Prefix futile rate | Zero futile rate | Prefix route expansions | Zero route expansions |
| --- | ---: | ---: | ---: | ---: |
| Seven facilities | 77.0508% | 77.3193% | 209,795 | 215,602 |
| 59 facilities | 83.4473% | 84.0088% | 125,715 | 118,691 |

Fewer futile proposals do not guarantee less routing work. The target prefix arm consumes
more route expansions despite its slightly lower futile fraction and identical paired
quality. Full invalid/unchanged/revisited and work ledgers remain in every episode summary.

## Independent batch-gradient observations

| Root / estimator | Mean gradient norm | Sample covariance trace | Mean norm / RMS batch dispersion |
| --- | ---: | ---: | ---: |
| Seven / prefix | 0.01613160 | 0.000868968 | 0.54724 |
| Seven / same-outcome terminal | 0.01250300 | 0.000678430 | 0.48002 |
| 59 / prefix | 0.00719873 | 0.000408140 | 0.35633 |
| 59 / same-outcome terminal | 0.00778961 | 0.000666079 | 0.30182 |

Covariance uses B-1 normalization over eight independent groups. The prefix/terminal trace
ratio is 1.28085 on the small root and 0.61275 on the target: prefix subtraction increases
observed dispersion in one case and decreases it in the other. This is not a general
variance-reduction theorem or a precisely estimated population SNR.

Seven of eight small-root prefix gradients have positive cosine to the mean of the other
seven batches; the eighth is -0.56857. Target prefix leave-one-out cosines range from
-0.19081 to 0.37338, with three negative. These measurements distinguish variable estimated
directions from missing or exactly zero task gradients; they do not prove which direction
improves longer-run layout quality.

## Important reassignment-control limitation

The cyclically reassigned rank credit is a misassignment control, **not an independent
zero-mean noise control**. Each peer's tournament rank includes comparisons against the
receiving worker, so dependence survives reassignment. This was identified during result
review; the preregistered allocation and outcomes were not changed or rerun.

For exchangeable independent four-worker trajectories, let D be a correctly assigned
expected raw gradient contribution and O the contribution using any fixed other worker's
credit. Rank credits sum to zero at every step, including prefix subtraction. Therefore
`D + 3 O = 0`, giving `O = -D/3`. This statement is before clipping and nonlinear Adam;
it is not a claim that observed finite batches or final quality are negatives of one another.
An added exhaustive reference test checks the actual cyclic assignment on all 256 joint
groups for each of three logit settings and two quality/futility cases, matching this
relation to 1e-12. The zero arm remains the clean no-current-update control here.
Any future independent-noise experiment must draw credit independently of the receiving
group or explicitly account for tournament coupling, rather than calling peer shuffling null.

## Direct visual inspection

The inspection selects the largest prefix-versus-zero improvement and regression by paired
area/tile/turn difference per root, with deterministic earliest ties and duplicate removal.
This is a post-hoc extrema view, not a representative or unbiased effect sample. Three
pairs remain. Their six full witnesses were freshly validated again; score and XY-density
metrics reproduce directly from geometry. The main agent inspected all three light-theme
screenshots and the target in dark theme, not just browser assertions.

- **Small-root largest paired benefit, B8/E3:** `[567,86,26]` becomes `[420,135,42]`.
  F03 moves below F01, and F02 occupies the lower-left area that was largely empty in the
  control. The box genuinely narrows from 27 to 21 cells. However, belts now wrap around
  F02 and run through longer central corridors; transport rises by 49 tiles and turns by
  16. This is an area-first win, not clean short routing. The drop in XY empty fraction
  from 57.7% to 33.8% reflects both a smaller box and added occupied transport cells.
  Since occupancy was not rewarded, this is not evidence of learned density-reward hacking.
  Visible detours warrant investigation, but neither appearance nor obstacle-free route
  distance proves which path is removable under fixed ports and obstacles.
- **Small-root largest paired regression, B4/E2:** `[480,105,30]` versus `[546,125,33]`.
  The prefix result is wider, with more central whitespace and a longer belt around F02.
  All three configured quality fields are worse. Retaining this case prevents the strongest
  positive image from standing in for the complete trial.
- **Target tie, B1/E1:** both sides are `[2132,392,147]`, 41 by 52, with 29.7% empty XY
  space. Facility rows, central gaps, and right-side pipe corridors visibly coincide.
  There is no evidence here of learned dense repacking or the 1% criterion being reached.

Final view: `target/layout-direct-frozen-batch-visual-02/frozen-update-layouts.html`.
All three cases pass browser checks in light/dark at 360, 736 and 1,024 pixels, including
facility selection and visible geometry: eighteen checks. An initial renderer omitted the
template's density input and failed the browser check; the corrected builder recomputes
density from each witness. No experiment or layout was changed to repair the visualization.

## Verification and reproduction

- Twenty-five Python tests pass: seed-role separation, exact credit assignments, batch
  covariance, lexicographic pairing, bootstrap units, strict preregistered allocation,
  cyclic-rank dependence, and the prior decoder/update/Adam/credit reference suite.
- Fifteen Node tests, workspace check and workspace tests pass; two existing Rust tests
  remain ignored. No Rust source edits, new dependencies or production cutover.
- All sixteen prefix capsules reproduce during collection. Separate-process replay of the
  first small and target capsules passes again, 512 actions each; persisted report and
  semantic episode ledgers reproduce independently as described above.
- Compared with the accepted constructive-planner design and AGENTS policy, this diagnostic
  preserves complete requests, validated retained factories, area/tiles/turns ordering,
  routing semantics and the production/research boundary. Existing user edits are untouched.

```sh
uv run --frozen --project docs/benchmarks/layout-learning python docs/benchmarks/layout-direct/frozen_batch.py \
  --config docs/benchmarks/layout-direct/frozen-batch.json --output-directory target/layout-direct-frozen-batch-01
uv run --frozen --project docs/benchmarks/layout-learning python docs/benchmarks/layout-direct/replay.py \
  --capsule target/layout-direct-frozen-batch-01/capsule-r0-b0.json \
  --sha256 3b65aaec5dfcc3926c0f6138df15c6a6ade32228d3cdb6d360b026a34c572f03
uv run --frozen --project docs/benchmarks/layout-learning python docs/benchmarks/layout-direct/replay.py \
  --capsule target/layout-direct-frozen-batch-01/capsule-r1-b0.json \
  --sha256 ac93f008264203cdd90dae4b8cf3e8e34cbbe5d9d41b8183f9d6e0ad99b946e4
node docs/benchmarks/layout-direct/frozen-batch-visual.mjs --plan docs/benchmarks/layout-direct/sampling.json \
  --run target/layout-direct-frozen-batch-01 --output-directory target/layout-direct-frozen-batch-visual-02
```

Output directories are exclusive; use a new name for a new execution. Timing metadata makes
fresh capsule bytes differ even if the actions and numerical update reproduce exactly.
