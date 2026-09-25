# Horizon-12 on-policy primary result

Status: primary stage complete; seed stress and subsequent mechanism checks are
in progress. This report is not a production promotion decision.

## Question and method

Does the frozen-search horizon-12 gain survive three fresh on-policy training
generations and reduce loss of cases solved by the starting model? Compare the
candidate with the completed horizon-6 `current` control across three streams.
The paired arms use the same nine 32-case training panels, disjoint 64-case
diagnostic panel, initial outer weights, frozen lower model, original policy
target, 100 epochs/1600 updates, 128 simulations per move, 16 candidates,
eight anchors, root seed 20260922, and rank-index Gumbels. The candidate
changes the search horizon from six to 12 and updates the outer checkpoint's
matching search contract. Later collections come from each arm's own model.

The [protocol](2026-09-26-hierarchical-h12-onpolicy-protocol.json) fixes inputs,
hashes, and gates. Source commit `249d943d`; release binary SHA-256
`66f72a2dbdf45aadbc50e4dfe8f3634012ef5f5a76d8159c698de20f85d7a4fa`.
The [progress](2026-09-26-hierarchical-h12-onpolicy-progress.json) holds all
64-case success masks and collection, training, and report hashes. The
[W&B observer](2026-09-26-hierarchical-h12-onpolicy-wandb-observer.json)
finished with all nine records remotely acknowledged at
[the run](https://wandb.ai/acrylicshrimp-personal/endfield-aic-layout-gen/runs/b10b8da375651d19becd129d).

## Paired diagnostic results

The common starting weights completed 26/64 at horizon six and 33/64 at
horizon 12. Each generation below is evaluated on the same diagnostic panel;
`lost` counts failures among the 26 cases solved by the horizon-six starting
checkpoint.

| Stream | Generation | H12 completed | H6 completed | H12 lost | H6 lost |
| --- | ---: | ---: | ---: | ---: | ---: |
| s1 | 1 | 40 | 22 | 8 | 15 |
| s1 | 2 | 41 | 24 | 5 | 16 |
| s1 | 3 | 36 | 20 | 14 | 19 |
| s2 | 1 | 38 | 28 | 9 | 16 |
| s2 | 2 | 39 | 26 | 8 | 18 |
| s2 | 3 | 36 | 24 | 12 | 19 |
| s3 | 1 | 34 | 27 | 13 | 16 |
| s3 | 2 | 38 | 29 | 6 | 13 |
| s3 | 3 | 34 | 20 | 9 | 15 |

After generation three, completion advantages are 16, 12, and 14 cases;
mean 14/64. Fewer starting successes are lost by 5, 7, and 6 cases; mean
six. The generation-two-and-three advantages are 17, 16, 13, 12, 9, and 14
cases, all positive. The predeclared [primary summary](2026-09-26-hierarchical-h12-onpolicy-primary-summary.json)
passes every numeric primary condition.

The candidate is still nonmonotonic. Its final diagnostic completion falls
from generation two by 5, 3, and 4 cases across streams. Relative to the
horizon-12 starting checkpoint, final completion gains are only 3, 3, and 1
cases. The large diagonal contrast therefore includes immediate search
strength and may include resistance to the control arm's training decline;
it does not yet isolate improvement in learned weights. The predeclared
three-seed stress test is running. If it passes, a same-search two-by-two
comparison will separate model and search effects before lower training is
restored or an untouched held-out panel is consumed.

## Reproduction

Under the recorded server workspace and LibTorch environment, with the
protocol's retained inputs and release binary:

```sh
python3 tools/factory-edit-pilot/hierarchical_horizon_onpolicy.py run \
  --output artifacts/recovery-horizon-onpolicy-v2
python3 tools/factory-edit-pilot/hierarchical_horizon_onpolicy.py summarize-primary \
  --output artifacts/recovery-horizon-onpolicy-v2
```

The control masks and checkpoints are recorded in the retained horizon-six
on-policy study. Raw collections and evaluations remain under the server
artifact root while the stress and mechanism stages continue. A final
closeout will record their cleanup.
