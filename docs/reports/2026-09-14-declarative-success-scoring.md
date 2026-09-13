# Local success-experience scoring screen

## Finding

Rarity changes which facility-edit successes survive a small selection budget.
An exterior-compaction bonus adds useful discrimination in the small stratum,
but cannot replace rarity protection: none of the six retained large-factory
facility-edit successes has a positive exterior-compaction signal.

Unbounded geometry bonuses can overstate the actual improvement. A successful
12-label segment has a 16.99% exterior signal but only a 1.96% reduction in used
area. Capping the geometry signal by the validated gain moves this example from
rank 23 to rank 1,110 within the small stratum. This is a useful correction to a
candidate ranking rule, not evidence that a policy trained with it is better.

No training, reward, replay sampler, live retention policy, or running server
configuration was changed. This is an offline screen of the already retained
pool, not a simulation of the production eviction algorithm.

## Evidence and continuity

The frozen seed-29 checkpoint at update 1,410 has 4,096 retained success segments
and 4,606 changed-action labels. The capture reads each of its 35 referenced
source chunks once through the existing optimized `blueprint_ppo_store.load`.
It decoded 214,590,497 compressed source bytes in 16.483 seconds, without model
execution, factory regeneration, or native replay. Compact numeric facility
aliases retain identity within each segment; recorded footprints and rotations
come from the actual input, not hardcoded game data. The local input is 619,203
compressed bytes. Local analysis of all seven profiles at all three budgets took
0.124 seconds; this excludes capture and plotting.

Existing benefits retained in the diagnostic are the one-pass compressed loader,
following actual label references, the native validated-success boundary, whole
preparation segments, and separation of research from normal training. JSON
analysis needs no learner or GPU. The retained input is sufficient to rerun every
ranking and geometry plot after the live source files have been pruned.

| Frozen pool | Small | Large |
| --- | ---: | ---: |
| Segments | 2,048 | 2,048 |
| Deletion-only segments | 1,931 | 2,022 |
| Insertion preparation without facility edits | 40 | 20 |
| Facility edits ending with a deletion | 32 | 5 |
| Facility-edit final action | 45 | 1 |
| All segments containing facility edits | 77 | 6 |
| Positive center-movement signal | 40 | 1 |
| Positive exterior-compaction signal | 29 | 0 |

Segments run from the preceding area improvement to the next one and contain
only changed actions. They are not complete episodes, and a final deletion does
not imply deletion-only experience. Already evicted examples and unsuccessful
attempts are outside this sample. The pool contains entries admitted at updates
1,376 through 1,410; it is not a representative sample of the full training run.

## Scoring definition

Only recorded native-validated, strictly area-improving segments are eligible.
The diagnostic checks this boundary and preserves all preparation labels,
including temporarily invalid intermediate geometry. There is no requirement
that each individual preparation action immediately improve the area.

Let `g = (preceding_best_area - successful_area) / preceding_best_area`.
This incremental gain is normalized by the preceding best, not the episode's
initial area, and must not be compared directly with evaluation area reduction.
For a segment containing a facility edit, `r = 1 - place_segment_fraction` in
its frozen product stratum; otherwise `r = 0`. Rarity is only a category proxy,
not proof that the edited facility was causally necessary for success.

`c` is movement toward the initial facility-bounding-box center, normalized by
the diagonal of the initial used extent. It accumulates signed distance changes
per facility and then uses the largest positive facility total. Thus simple
out-and-back movement cancels, and editing more facilities does not by itself
increase the bonus. This center is a geometric heuristic, not a transport-aware
optimal target.

`e` is the positive part of the signed facility-bounding-box area reductions
across the recorded `place` transitions, divided by preceding best used area.
It uses actual rotated body dimensions and both sides of each axis. Band
transformations are not counted as facility-edit compaction. Both signals are
bounded by one and invariant to translating the entire geometry.

The native validator canonicalizes coordinates using all referenced geometry
before measuring occupied used extent. Capture accounts for that origin shift;
raw witness coordinates are not mistaken for canonical width and height.
Transport cells determine the successful used area, while `e` measures facility
bodies only. Consequently `e` can exceed `g`, especially when insertions and
deletions are interleaved with facility edits. Neither signal establishes a
counterfactual contribution to success.

The tested score is `g + rare_weight*r + center_weight*c + exterior_weight*e`.
The capped exterior profile replaces `e` with `min(e, g)`. The checked-in JSON
sets all weights explicitly. No coefficient was selected using evaluation
returns or adopted in production.

## Equal-budget ranking results

Each row independently selects the top 128 segments from each 2,048-entry
stratum, with stable identity tie breaking. This hypothetical selection budget
is not a new buffer capacity. Parentheses show mean incremental validated area
gain among selected segments, not post-training performance.

| Profile | Small facility-edit segments / 128 | Large facility-edit segments / 128 |
| --- | ---: | ---: |
| Area only | 5 (18.044%) | 0 (4.164%) |
| Area + up to 1 percentage point rarity | 6 (18.040%) | 1 (4.159%) |
| Area + up to 3 points rarity | 12 (17.946%) | 6 (4.093%) |
| Area + up to 10 points rarity | 44 (16.161%) | 6 (4.093%) |
| 3-point rarity + 0.1 × center movement | 13 (17.918%) | 6 (4.093%) |
| 3-point rarity + uncapped exterior | 26 (17.140%) | 6 (4.093%) |
| 3-point rarity + capped exterior | 24 (17.319%) | 6 (4.093%) |

At 512 per stratum, area-only / 3-point rarity / capped exterior retain
18 / 36 / 47 small facility-edit segments and 1 / 6 / 6 large ones.
At 1,024 per stratum the corresponding counts are 40 / 69 / 69 and 4 / 6 / 6.
The exterior bonus therefore changes selection most at tight budgets; its
incremental effect disappears at the largest tested budget. Higher rarity
weights retain more rare examples while sacrificing some immediate gain.

## Inspected geometry examples

The local gallery contains the exact before/final facility bodies, changed
facility aliases, action order, and source label references for four cases:

| Case ID prefix | Actual area | Exterior signal | Interpretation |
| --- | --- | ---: | --- |
| `7ca69548` | 176 → 160 | 17.05% | One facility-edit action narrows the facility envelope; actual gain is 9.09%. |
| `28c3cd68` | 153 → 150 | 16.99% | A mixed 12-label sequence gets excessive uncapped geometry credit for a 1.96% actual gain. |
| `9b14fd2f` | 3,520 → 3,410 | 0% | Large-factory preparation edits end with a deletion and improve area by 3.125%. |
| `49f420e2` | 2,610 → 2,565 | 0% | Two facility edits improve area by 1.724% without shrinking the facility envelope. |

The plots deliberately omit transport paths; their area annotations use the
complete recorded witness. A facility-only picture does not prove why a path
became shorter or whether an earlier move was necessary. The results support
testing rarity protection with a bounded spatial bonus, rather than making
inward movement a mandatory admission condition.

## Reproduction and verification

Retained local input and outputs are under
`artifacts/declarative-success-scoring/20260914/`. The companion provenance file
binds the compact input and configuration. Regenerable plots and full rankings
are not tracked.

```sh
python3 tools/factory-edit-pilot/diagnostics/declarative_success_scoring.py analyze \
  --input artifacts/declarative-success-scoring/20260914/input.json.gz \
  --config tools/factory-edit-pilot/diagnostics/declarative-success-scoring.json \
  --output artifacts/declarative-success-scoring/20260914/report.json

tools/factory-edit-pilot/.venv/bin/python \
  tools/factory-edit-pilot/diagnostics/plot_declarative_success_scoring.py \
  --input artifacts/declarative-success-scoring/20260914/input.json.gz \
  --report artifacts/declarative-success-scoring/20260914/report.json \
  --output artifacts/declarative-success-scoring/20260914

python3 -m unittest discover -s tools/factory-edit-pilot/diagnostics \
  -p test_declarative_success_scoring.py
```

A new capture can be made with the same scorer's explicit `capture --run
<seed-directory> --runtime <existing-pilot-source> --output <input.json.gz>`
subcommand. It only reads the published checkpoint and referenced source files.
The run's parent directory must contain its bound `launch.json`.

Ten local tests cover interior-versus-exterior moves, inverse moves, rotated
footprints, canonical translation, transport-determined extent, invalid success
rejection, unrelated-facility changes, score ablation, and the actual-gain cap.
The actual CLI and plots were inspected. `cargo check --workspace` and
`cargo test --workspace` passed; three native tests requiring external retained
inputs were ignored by their existing declarations. Repository hygiene checks
passed for worktree and staged content.
