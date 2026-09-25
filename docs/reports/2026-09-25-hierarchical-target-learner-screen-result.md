# Frozen-collection policy-target learner screen

Status: all nine prespecified training arms and ten common diagnostic searches
completed. W&B acknowledged the readiness record before training and all nine
final arm records. This screen selects the scope for a separate fresh-collection
confirmation; it is not a production learner change.

## Method and input isolation

Three disjoint frozen 16-case search collections were reused. Each stream
trained `current`, `contrast-first`, and `contrast-all` independently from the
same generation-87 weights, with the lower model frozen, 100 epochs, 256 outer
states per epoch, batch size 16, and 1,600 optimizer updates per arm. The
contrast was the predeclared half-visited **stress** target because no offline
alternative passed the replicated-outcome loss gate. The three arms within a
stream had the same report except for the target fields selected by scope.
One integrated semantic comparison of S1's first-only synthetic report found
16 changed first roots and 43,204 changed action probabilities; after restoring
those probabilities, every case and report field equaled the source report.
The 11 new panels plus the original 48 cases represented 464 distinct
canonical problems. The two 64-case diagnostic panels are distinct from all
training panels; this screen used the first diagnostic panel only.

## Result

The starting model completed 23/64 diagnostic cases: four easy and 19 hard.
`Gained` and `lost` compare each arm's case mask to that same starting model.

| Stream | Target scope | Completed /64 | Gained | Lost | Easy / hard completed |
| --- | --- | ---: | ---: | ---: | ---: |
| S1 | Current | 18 | 12 | 17 | 6 / 12 |
| S1 | First only | 23 | 13 | 13 | 7 / 16 |
| S1 | All roots | 20 | 13 | 16 | 7 / 13 |
| S2 | Current | 22 | 11 | 12 | 5 / 17 |
| S2 | First only | 25 | 16 | 14 | 7 / 18 |
| S2 | All roots | 23 | 15 | 15 | 5 / 18 |
| S3 | Current | 19 | 13 | 17 | 6 / 13 |
| S3 | First only | 17 | 9 | 15 | 5 / 12 |
| S3 | All roots | 20 | 11 | 14 | 5 / 15 |

Relative to the paired current arms, first-only reduced lost starting
successes by `[4, -2, 2]`, a mean of 1.33 cases, and changed completions by
`[+5, +3, -2]`. All-root changed retention by `[+1, -3, +3]`, a mean of 0.33,
and completions by `[+2, +1, +1]`. Both scopes improved retention in two of
three streams, but neither reached the predeclared **three fewer losses on
average**. The first-only variant was chosen only as the fixed stress scope
because the protocol specifies that fallback; there is no screen efficacy
claim. Large simultaneous gains and losses persist in every arm, and the S2
first-only arm illustrates why aggregate completion alone is insufficient:
it completed three more cases than current while losing two more starting
successes.

All nine training reports passed their source collection, starting model,
configuration, epoch, and update-count checks. Their losses decreased within
each arm, but losses trained on different target distributions are not a
cross-arm quality metric. Diagnostic case masks, easy/hard counts, conditional
quality, model hashes, and timing are retained in the
[progress](2026-09-25-hierarchical-target-learner-screen-progress.json).
This frozen-collection screen isolates the immediate target change. It cannot
measure the feedback from collecting new on-policy states or multigeneration
retention; the next experiment addresses those separately.

## Provenance and closeout

The [protocol](2026-09-25-hierarchical-target-learner-screen-protocol.json)
hashes every panel, three original reports, model checkpoint, binary,
configuration, runner, transformer, and tracking code. It records source
experiment commit `a50383b5`. The retained [W&B observer
status](2026-09-25-hierarchical-target-learner-screen-wandb.json) confirms
remote step 9 and links the run:
[target learner screen](https://wandb.ai/acrylicshrimp-personal/endfield-aic-layout-gen/runs/0a1b4309f818237ec0246892).
The nine training calls took 52 seconds; ten 64-case diagnostic searches took
2,188 seconds and dominated runtime. An initial panel-preparation attempt
failed before producing a panel because the documented libtorch shared-library
path was omitted; the error log was retained and preparation was repeated
with the required environment. No protocol input or treatment changed.

After this report and case masks are verified, the
[cleanup manifest](2026-09-25-hierarchical-target-learner-screen-cleanup.json)
records removal of the screen's regenerable reports, transformed inputs,
diagnostics, and training checkpoints. The generated 11 panel files and their
protocol hashes remain until the explicitly planned confirmation finishes.

With the documented libtorch environment exported under `~/Srv` and the
nonblocking observer remotely ready:

```bash
python3 tools/factory-edit-pilot/hierarchical_target_learner.py prepare \
  --output artifacts/target-causal-study --experiment-commit a50383b5
python3 tools/factory-edit-pilot/hierarchical_target_learner.py screen \
  --output artifacts/target-causal-study
```
