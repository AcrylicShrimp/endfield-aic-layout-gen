# 64-update checkpoint in the ongoing cumulative study

Status: all three 64-update evaluations verified; 256/1024 allocations still running.
Date: 2026-09-07. Method and stopping rule: `budget-curve-contract.md`.

## Decision

The first fixed checkpoint has positive observed development win-minus-loss estimates for
all three training seeds, against both initial and conditional-uniform policies. This is
encouraging finite-sample evidence, not a precise population estimate or a completed learning
curve. The target still ties its controls on every matched evaluation, with unchanged area and
29.6904% empty XY space. Development futile actions remain about 79-80% and increase slightly
on average against the initial policies. Neither production readiness nor insufficient-training
exclusion is established. Continue the unchanged learners to the declared 256/1024 checkpoints;
do not use these early outcomes to stop, select a checkpoint, change reward, or enlarge the model.

This is a new experiment on the current broader-load stream, not a causal comparison with the
historical 64-update runs. Within this study, method and evaluation stay fixed while cumulative
training grows. More updates beyond 64 have begun, but their prospective evaluations are not
yet available. No claim about that unmeasured portion of the curve follows from this report.

## Prospective development comparison

Each seed evaluates twelve identical frozen validation factories with two matched sampling
seeds, 1024 proposals per episode. Scores compare area, physical tiles and turns lexicographically.
Wins/losses below are trained versus the named control; ties contribute zero to the endpoint.

| Training seed | Against initial: wins / losses / ties | Against uniform: wins / losses / ties | Initial-control paired endpoint | Mean area / tiles / turns difference from initial |
| --- | --- | --- | ---: | --- |
| 1901 | 13 / 9 / 2 | 14 / 8 / 2 | 0.16667 | -24.7083 / -11.4583 / -1.2500 |
| 2901 | 13 / 9 / 2 | 16 / 6 / 2 | 0.16667 | +1.2500 / +6.4583 / +0.2083 |
| 3901 | 14 / 9 / 1 | 13 / 9 / 2 | 0.20833 | -19.5000 / -9.1250 / -0.8333 |

The three-seed mean endpoint is 0.18056, range 0.16667-0.20833. Combined descriptive counts
are 40 wins, 27 losses and five ties against initial; 43/23/6 against uniform. Do not treat the
72 episodes or their 73,728 action steps as independent trained-model replicates: the independent
training unit is the seed, all seeds share one small development population, and there are only
three seeds. No narrow population interval or arbitrary-factory effectiveness claim is made.
Seed 2901 illustrates that more lexicographic wins can coexist with worse average component
differences when a few losses are larger. The objective has not become a weighted sum.

| Seed | Initial futile % | Trained futile % | Initial route expansions | Trained route expansions |
| --- | ---: | ---: | ---: | ---: |
| 1901 | 79.3579 | 79.1789 | 870,198 | 665,541 |
| 2901 | 78.9185 | 79.6956 | 783,234 | 757,097 |
| 3901 | 79.1056 | 79.2318 | 786,196 | 721,980 |

Mean development futile fraction worsens by 0.2414 percentage points against initial, despite
lower observed route expansions in each seed. Controls match proposal counts and per-action
route bounds, not consumed work. Co-running inference/elapsed times are retained in the episode
records but are not isolated throughput benchmarks.

## Full target, reported separately

All six trained target evaluations and their initial/uniform counterparts end at
`[2132,392,147]`, 41 by 52, with 29.6904% empty XY area. There are zero target wins or losses
at this checkpoint. Target training trajectories sometimes reduce wiring, but none of those
training outcomes should be substituted for these independently sampled prospective outcomes.

| Seed | Initial futile % | Trained futile % | Initial route expansions | Trained route expansions |
| --- | ---: | ---: | ---: | ---: |
| 1901 | 84.8633 | 83.4961 | 35,643 | 48,643 |
| 2901 | 85.3516 | 85.4004 | 49,943 | 52,471 |
| 3901 | 84.6191 | 84.9609 | 44,451 | 45,763 |

The target's slightly lower mean futile fraction does not imply less routing work or improved
packing. All three trained target policies consume more route expansions than their initial
controls, and the 1% density criterion is still unmet.

## Actual streaming exposure through update 64

Across the three prefixes, 1,185 generation attempts produce 264 complete factories. Of these,
96 single-facility factories are explicitly learning-ineligible and 168 enter training. Other
outcomes are 667 flow rejections, 237 construction stops, fourteen duplicates and three
construction-budget stops. None is repaired by cropping or a stored-root fallback. There are
no observation-support exclusions in these prefixes. Each seed has 56 fresh training groups
and eight declared replays of the original full target: 192 groups and 393,216 training actions.

| Seed | Generation attempts | Fresh groups | Fresh medium groups | Largest fresh factory | Distinct training target families |
| --- | ---: | ---: | ---: | ---: | ---: |
| 1901 | 403 | 56 | 6 | 52 | 43 |
| 2901 | 436 | 56 | 3 | 31 | 42 |
| 3901 | 346 | 56 | 3 | 44 | 46 |

Unlike the historical small-load runs, this stream really trains on twelve medium factories,
including 44- and 52-facility roots. All twelve have one internal weak component; examples
include a 23-facility graph with a five-facility cycle and a 52-facility graph with eleven
three-facility cycles. These cases do not require or approve the separate parallel-component
admission proposal. The current constructor can handle some connected medium graphs while
still stopping on other requests. The complete validation population remains 2-14 facilities
plus the explicitly trained target, so there is no held-out medium/large generalization result.

The three streams cover 84 training target item families and 158 distinct recorded production
fingerprints, not 168 independent production problems: cross-seed repeats exist. These hashes
are not a graph-isomorphism certificate. No newly generated 65-128-facility factory enters
training in these prefixes; three solved large requests appear among generation attempts.

## Direct geometric inspection

The declared first seed and first evaluation sampling seed are used for the full target and
the largest frozen validation root, selected before learning. These are fixed diagnostic cases,
not the most flattering results. The final-budget post-hoc regression view remains deferred
until every 1024-update run finishes. Its deterministic rule is greatest positive area/tile/turn
difference from the matched initial policy, breaking ties by training seed, root ID and sampling
seed; the label remains "least favorable" if no actual regression occurs.

The main agent directly inspected both paired light-theme screenshots and the 14-facility pair
in dark theme. All four selected witnesses pass fresh original full-request coverage and geometry
audits, and their scores/densities reproduce from their actual cells.

- **Target:** the layouts coincide, including facility rows, central voids and right-side pipe
  corridors. This is visibly unchanged packing, not an area reduction hidden by summary metrics.
- **Fourteen facilities:** `[1400,518,91]` becomes `[1295,453,82]`, narrowing 40 to 37 columns
  at unchanged height 35. F03 moves from upper-left to lower-left, F06 moves upward, and the
  right-hand facilities rearrange. The long belt loop on the left is reduced; total transport
  and turns also fall. Nevertheless, a large upper-left empty region and long central/right
  corridors remain. This is a real score improvement, not dense or demonstrably nonredundant
  routing. Obstacle-free chain excess falls from 54 to 30 but is not a removable-route proof.
  Empty fraction rises from 49.7143% to 50.6564% because transport occupancy shrinks too; using
  empty percentage alone would miss the simultaneous improvement in all configured score fields.

Both sides use the same tile scale. For later multi-checkpoint views, every displayed checkpoint
of a root shares the maximum display extent; unused display grid outside the outlined used box
does not become factory footprint. The view passes twelve browser checks: two cases, light/dark,
360/736/1024 widths, exact facility counts, visible geometry, readable labels, selection, and
matching tile scales. Four additional Node tests cover fixed selection, final extrema, incomplete
evaluation rejection and shared bounds without changing score metrics.

View: `target/layout-direct-budget-visual-64-01/cumulative-layouts.html`, 104,206 bytes.
No training source was edited or experiment rerun to create this view.

## Evidence integrity and reproduction

All 768 training and 234 evaluation episode artifacts in the declared prefixes were independently
read and hash-checked, their semantic summaries and exact 512/1024 trace lengths reproduced, and
both audit layers checked. That is 632,832 executed proposals at this measurement boundary, not
the continually increasing live total. All three model/Adam/RNG state chains remain continuous
through evaluation and into update 65. Updates 1 and 64 per seed pass exact full-capsule replay;
other intermediate updates are not claimed to be fully replayable from saved observations.

Uniform controls additionally reproduce identical action sequences, final witness hashes, waste
and work counts across all three seeds (78 episodes, 79,872 proposals). A first ad hoc assertion
incorrectly compared wall-time fields too; excluding only `_ms` work fields makes the intended
semantic comparison pass. Different timings are expected, not an environment determinism bug.

All frozen source hashes still match. Thirty-five Python tests, nineteen direct Node tests,
workspace check and workspace tests pass; two existing Rust tests remain ignored. No Rust edits
or new dependencies. This visual/measurement slice preserves the accepted connected constructive
design, immediate routing, complete retained factories, score order and production/research boundary.

| Seed | 64-update evaluation SHA-256 |
| --- | --- |
| 1901 | `8e78869dad8d486efe3d70671ead8761fe34c299799188deecd832f6fbadcff6` |
| 2901 | `e648484f7e4c5c37715b6e027c92874cafcf880a661bf6deaf92618f3b50b45e` |
| 3901 | `68b14d00e81283a8fa1726a2a992c2fbfa057f81b73acaa89e243b1e6acd33a4` |

These files are `evaluation-0064-trained.json` in the respective seed directories listed in
`budget-curve-progress.md`. The cumulative experiment remains running; do not restart it to
reproduce the visual.

```sh
node docs/benchmarks/layout-direct/budget-curve-visual.mjs --plan docs/benchmarks/layout-direct/sampling.json \
  --runs target/layout-direct-budget-seed-1901-02 --checkpoint 64 \
  --output-directory target/layout-direct-budget-visual-64-01
```

Use a new exclusive output name for another rendering. Final allocation analysis still requires
all three completed 1024-update reports, as specified in the original contract.
