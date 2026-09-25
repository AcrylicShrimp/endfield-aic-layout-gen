# Fresh-collection policy-target learner confirmation

Status: all 18 prespecified arms completed, and W&B remotely acknowledged all
18 results. The half-visited first-root target was a **stress contrast** after
both the independent-outcome and frozen-collection selection gates failed.
This confirmation did not establish a learner improvement or change a
production setting.

## Method

Three independent streams started from the same generation-87 weights. Each
stream trained `current` and `contrast` for three generations on common new
32-case training panels. The lower model stayed frozen. Every arm used the
same 16-candidate/128-simulation search, 100 epochs, 256 sampled states per
epoch, batch 16, 1,600 updates, optimizer settings, and common 64-case
diagnostic. Generation 1 reused one byte-identical search collection within
each pair; only the contrast's first-root policy targets changed. Generations
2–3 recollected search data separately from each arm's preceding outer
checkpoint. The protocol and progress validated panel, collection, model,
search, training and evaluation hashes for each arm.

## Diagnostic result and declared gate

The starting model completed 22/64 cases, including six easy and 16 hard.
Each cell below is the count after generations 1 → 2 → 3; `lost` means
starting-model successes that the final model no longer completed.

| Stream | Current completions | Contrast completions | Final current lost | Final contrast lost | Final completion difference |
| --- | --- | --- | ---: | ---: | ---: |
| S1 | 23 → 30 → 21 | 16 → 25 → 21 | 15 | 14 | 0 |
| S2 | 22 → 21 → 19 | 17 → 27 → 21 | 18 | 15 | +2 |
| S3 | 25 → 23 → 17 | 20 → 17 → 25 | 12 | 10 | +8 |

At generation 3, the contrast lost `[1, 3, 2]` fewer starting successes
than current, a mean improvement of **2.0 cases**. All three streams improved
in that narrow metric, and total completions improved by `[0, 2, 8]`, a
mean of 3.33. The predeclared retention gate required **at least three fewer
losses on average**, positive retention in two streams, and no more than two
fewer total completions on average. It **failed** the mean-retention
requirement. The large S3 aggregate advantage is partly a rebound of the
contrast from 17 to 25 completions while current fell from 23 to 17; it does
not show stable monotonic learning. At generation 1, when collection input
was shared and only target fields differed, contrast completed 7, 5 and 5
fewer cases than current in S1–S3 respectively.

Case-level churn remains large in both arms. S1 current fell from 30 to 21
between generations 2 and 3 by losing **21** previously solved cases and
gaining 12 others; S1 contrast lost 17 and gained 13 on the same transition.
S3 contrast went 20 → 17 → 25, not steadily upward. At the final generation,
the two arms both solved only 9 cases in S1, 8 in S2, and 7 in S3; each
stream also had many arm-specific successes. Final easy/hard completions were
S1 current 6/15 versus contrast 5/16, S2 4/15 versus 4/17, and S3 6/11
versus 6/19. The [analysis](2026-09-25-hierarchical-target-learner-confirmation-analysis.json)
and [case masks and lineage](2026-09-25-hierarchical-target-learner-confirmation-progress.json)
retain all trajectories, transitions, per-case outcomes and conditional
layout-quality summaries. Conditional quality compares different success
sets and cannot establish an overall quality benefit.

## On-policy feedback and interpretation

The [feedback analysis](2026-09-25-hierarchical-target-learner-feedback-analysis.json)
verified that every paired collection used identical 32 case identities and
first-state hashes. Generation 1 shared the exact search report and therefore
all 32 selected first actions. At generation 2, only **1, 0 and 2 of 32**
first actions matched between current and contrast in S1–S3. At generation 3,
**none** matched in any stream. Source-search completions were similar but
variable: S1 current/contrast `14/14 → 18/17 → 9/11`, S2
`12/12 → 13/12 → 14/16`, and S3 `16/16 → 13/15 → 10/8`.
The first-root target treatment can therefore change subsequent search data
strongly after one learner update. This is a measured feedback path, not proof
that it alone caused the original curriculum oscillation: the current arm
also changed substantially across fresh collections, and downstream MCTS
seed sensitivity and short-horizon Q mismatch were separately observed.

The tested half-visited transform was selected as a stress contrast precisely
because it worsened the independent-action offline loss and failed the frozen
screen. These data reject its promotion under the declared gate. They do not
establish that every possible target construction would fail, nor do they
isolate an optimal candidate cap or full-continuation value estimator.

## Provenance, runtime, and closeout

The [protocol](2026-09-25-hierarchical-target-learner-confirmation-protocol.json)
contains the source experiment commit `84191214`, screen choice and hashes,
all 11 panel hashes, and binary/configuration/runner/transform/tracking hashes.
The [W&B observer status](2026-09-25-hierarchical-target-learner-confirmation-wandb.json)
records an acknowledged readiness step before the run and final remote step
18: [target learner confirmation](https://wandb.ai/acrylicshrimp-personal/endfield-aic-layout-gen/runs/e7935a1228757a769ca996c7).
The integrated calls spent 1,457 seconds collecting search reports,
94 seconds training, and 4,241 seconds evaluating the common diagnostic;
evaluation dominated total runtime. The declared-gate analysis code is commit
`dbb610db4f563d18f3dbf068af9fec6bc2d40a46` (SHA-256
`598b0787efad824d14674d4cc9df47b3df6bd3b1c40bd13ce027bec73209d3c2`).
The final feedback analysis code is commit
`d4f6efa69db75a4d721b54212e62e2ad6987fab3` (SHA-256
`3172fe4af7fed03140e7519496333cbc3d7c197561a57b864a7c2a2386063000`).
These analysis additions did not alter the running learner or its inputs.

After this package is verified, the
[cleanup manifest](2026-09-25-hierarchical-target-learner-confirmation-cleanup.json)
records removal of regenerable training reports, intermediate models,
diagnostic reports, and telemetry. Source inputs and any checkpoint needed
elsewhere are reviewed separately before removal.

With the documented libtorch environment exported under `~/Srv` and the
nonblocking W&B observer remotely ready:

```bash
python3 tools/factory-edit-pilot/hierarchical_target_learner.py prepare-confirmation \
  --output artifacts/target-causal-study --experiment-commit 84191214
python3 tools/factory-edit-pilot/hierarchical_target_learner.py confirmation \
  --output artifacts/target-causal-study
python3 tools/factory-edit-pilot/analyze_hierarchical_target_learner_confirmation.py \
  --output artifacts/target-causal-study
python3 tools/factory-edit-pilot/hierarchical_target_feedback_analysis.py \
  --output artifacts/target-causal-study
```
