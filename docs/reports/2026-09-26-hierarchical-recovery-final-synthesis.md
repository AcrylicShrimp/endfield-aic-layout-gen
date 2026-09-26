# Hierarchical Gumbel-MCTS recovery: final research synthesis

Status: a search/training setup with materially better completion and
retention is validated for the tested bounded two-facility distribution.
The remaining learner instability is identified as a separate research
question; the evidence does not support a claim of monotonic policy learning
or generalization to larger factories.

## Tested explanations and decisions

| Hypothesis | Controlled evidence | Decision |
| --- | --- | --- |
| The 16-candidate root cap misses useful actions. | [Causal study](2026-09-25-hierarchical-target-causal-study-result.md): independently confirmed useful outside actions exist, but widening the cap under one seed did not reliably recover them. | Real limitation; widening alone is not a validated fix. |
| Short search and finite simulation budget make Q and targets unreliable. | [Causal study](2026-09-25-hierarchical-target-causal-study-result.md): searched Q misranked independently confirmed first-action pairs; 512 simulations improved some outcomes but left substantial seed sensitivity. | More simulations alone do not establish a stable learner target. |
| A diffuse policy target causes the churn and can be fixed by sharpening. | [Target audit](2026-09-25-hierarchical-target-quality-audit-result.md) measured about 99.5% first-root mass on unvisited actions. Sharper/visited-focused targets failed confirmed-action scoring or the fresh learner gate; [successful-action imitation](2026-09-26-hierarchical-success-onpolicy-result.md) failed four of five fresh on-policy promotion checks. | Dilution is measured; the tested target changes are rejected. |
| Case-ID-dependent Gumbel assignment explains namespace sensitivity. | [Paired namespace confirmation](2026-09-25-hierarchical-horizon-strict-result.md) changed absolute completions on identical graphs. [Physical-action keying](2026-09-26-hierarchical-action-key-eight-result.md) removed ID churn in eight cases but reduced completion and increased runtime. | Mechanism confirmed; keyed variant is not a performance setup. |
| Longer receding-horizon search improves robust completion. | [Frozen confirmation](2026-09-25-hierarchical-horizon-strict-result.md) gained 12/14/14 completions per 64 across three seeds under the declared namespace. The [on-policy candidate selection](2026-09-26-hierarchical-h12-mixed-seed-result.md) preferred H12-trained/H12-search over H6-trained/H12-search on new seeds. | Select H12/H12 for the fresh lower-trained restart. |
| The gain survives lower training and independent evaluation. | [Fresh lower-trained primary](2026-09-26-hierarchical-lower-restored-primary-result.md) passed five fixed checks across three streams. [Reserved held-out](2026-09-26-hierarchical-lower-restored-heldout-result.md) passed four fixed checks across nine stream-seed pairs. | Validated integrated setup on the bounded distribution. |
| H12-trained weights themselves steadily improve. | [Post-held-out factorial](2026-09-26-hierarchical-lower-outer-cross-result.md) held H12 search fixed: lower and outer model-origin effects changed sign across streams and averaged slightly negative. | Not established. Do not attribute the integrated gain to monotonic learning. |

## Validated setup and scope

Use the exact [H12 search configuration](../benchmarks/hierarchical-target-causal/horizon-12-search.json)
with 128 simulations per move, 16 root candidates, eight anchor candidates,
24 maximum moves, and a matching outer-checkpoint search contract. The tested
training setup used 100 epochs, the original policy target, and updates to
both lower and outer models on fresh on-policy collections. The H12 search
configuration differs from the H6 control only in the horizon value.

On the untouched 128-case panel, H6/H6 averages `45.67` completions and
H12/H12 averages `77.56`: **+31.89 completions per 128** over nine paired
stream-seed evaluations. All nine gains are positive (minimum `+16`). H12
loses **9.56 fewer starting successes** on average; eight pairs improve
retention and one ties. Among 255 common-success pair instances, H12 layouts
have mean paired footprint `26.18` units smaller and use `6.54` fewer
transport cells. Mean evaluation wall time rises from `359.21` to `361.39`
seconds (`+0.61%`) in this run. These are conditional quality and runtime
measurements on the tested panel, not universal cost guarantees.

The validation covers generated two-facility cases with at most six
connections. It establishes an evidence-backed production setup **proposal**
for that scope; it does not silently change production defaults or claim that
larger factory layouts have been validated. The original oscillation remains
possible because each generation changes search trajectories and training
targets. The fixed-search factorial shows that neither learned component has
a consistently positive effect, so a separate learner-target study tied to
independently validated continuation outcomes is needed if monotonic learning
is the next objective. Keep that study separate from this completed
candidate-selection and held-out verdict.

Direct inspection of the archived panels confirms 480 distinct problem-graph
identities: 288 training cases across nine panels, 64 diagnostic cases, and
128 held-out cases. The held-out partition is marked `held-out` in its saved
panel; the training configuration hash matches the tracked 100-epoch config
with `train_lower: true`.

## Provenance

The primary [protocol](2026-09-26-hierarchical-lower-restored-protocol.json),
[progress](2026-09-26-hierarchical-lower-restored-progress.json), and
[W&B acknowledgement](2026-09-26-hierarchical-lower-restored-wandb-observer.json)
bind the fresh on-policy run. The held-out [protocol](2026-09-26-hierarchical-lower-restored-heldout-protocol.json),
[success masks](2026-09-26-hierarchical-lower-restored-heldout-progress.json),
and [summary](2026-09-26-hierarchical-lower-restored-heldout-summary.json)
bind the independent final gate. The factorial's
[protocol](2026-09-26-hierarchical-lower-outer-cross-protocol.json) and
[summary](2026-09-26-hierarchical-lower-outer-cross-summary.json) bind the
fixed-model diagnostic. Each stage report contains its reproduction commands,
source identity, interpretation, and cleanup manifest. All retained
repository files are English. After the attribution continuation closed, the
final checkpoints, exact panels, and search contracts were placed in an
ignored `artifacts/recovery-lower-restored-v2/final-evidence.tar.gz` copy in
both the local workspace and training-server workspace. The archive SHA-256 is
`488d68b57343550be13fca36f8bb0456ae32e5876811cdf62e528207f04232b3`;
the [file manifest](2026-09-26-hierarchical-recovery-final-evidence-files.json),
[package record](2026-09-26-hierarchical-recovery-final-evidence-package.json),
and [final cleanup](2026-09-26-hierarchical-recovery-final-closeout.json)
record verification and removal of the unpacked executable state. To restore
the retained inputs for a new study, extract the archive under its parent
`artifacts/recovery-lower-restored-v2` directory and verify the file hashes
against the manifest.
