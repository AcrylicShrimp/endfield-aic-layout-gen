# Restored lower training with horizon-12 search: held-out result

Status: all 21 prespecified evaluations and all four held-out promotion
checks pass. This supports a horizon-12 search setup on the tested two-facility
distribution. It does not establish monotonic learning or a positive effect
from the learned weights alone.

## Question and controlled method

After the [primary on-policy study](2026-09-26-hierarchical-lower-restored-primary-result.md)
passed its fixed gate, does its selected horizon-12 training/search setup
improve completed layouts and preservation of starting successes on the
untouched, disjoint 128-case held-out panel? Compare the three independently
trained horizon-six/H6-search finals and horizon-12/H12-search finals on the
same cases under root seeds 20260922, 20260923, and 20260924. Evaluate the
common starting H6 model once per seed to define that seed's retention mask.
No held-out outcome was used to select the candidate or adjust a threshold.

The [protocol](2026-09-26-hierarchical-lower-restored-heldout-protocol.json)
binds the parent study, frozen binary, reserved panel, final checkpoints,
seeded search configurations, runner, and numeric gate by SHA-256. The
[progress](2026-09-26-hierarchical-lower-restored-heldout-progress.json)
retains all 21 success masks, complete-evaluation report hashes, and runtimes.
The runner was introduced in commit `beb88374`; its SHA-256 in the protocol
matches the committed source. The parent training collected fresh on-policy
trajectories with lower-model training enabled. This held-out stage is a
search-only evaluation of the resulting checkpoints.

## Results

Each `completed/lost` cell gives validated completions out of 128 and losses
among cases completed by that seed's starting H6 model. A negative quality
difference means the H12 layout is smaller on cases where **both** arms
completed. `Common` is the number of those cases.

| Seed | Stream | H6 completed/lost | H12 completed/lost | Completion gain | Common | H12 minus H6 footprint | H12 minus H6 transport cells |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 20260922 | s1 | 44 / 29 | 90 / 12 | +46 | 32 | -19.03 | -11.03 |
| 20260922 | s2 | 40 / 31 | 78 / 15 | +38 | 24 | -59.33 | -11.83 |
| 20260922 | s3 | 54 / 28 | 75 / 17 | +21 | 39 | -16.69 | -2.72 |
| 20260923 | s1 | 43 / 30 | 82 / 20 | +39 | 26 | -23.42 | -0.42 |
| 20260923 | s2 | 42 / 30 | 81 / 18 | +39 | 24 | -28.63 | -2.67 |
| 20260923 | s3 | 47 / 27 | 63 / 27 | +16 | 25 | -34.56 | -10.72 |
| 20260924 | s1 | 44 / 27 | 75 / 20 | +31 | 26 | -23.96 | -12.23 |
| 20260924 | s2 | 50 / 25 | 76 / 21 | +26 | 30 | -27.13 | -2.50 |
| 20260924 | s3 | 47 / 27 | 78 / 18 | +31 | 29 | -13.62 | -6.48 |

H6 averages `45.67/128` completions; H12 averages `77.56/128`, a gain of
`31.89/128`. All nine pairs improve, with seed-mean gains `35.0`, `31.33`,
and `29.33`. The mean reduction in lost starting successes is `9.56` cases;
seed means are `14.67`, `7.33`, and `6.67`. Eight pairs improve retention and
one ties. The fixed thresholds were mean completion gain at least eight,
positive mean gain on at least two seeds, mean reduction in lost starting
successes at least six, and positive mean retention on at least two seeds.
All four [held-out checks](2026-09-26-hierarchical-lower-restored-heldout-summary.json)
pass without changing the gate.

On 255 common-success pair instances, the common-case weighted differences
are `-26.18` footprint units and `-6.54` transport cells. Each of the nine
pair means is negative on both metrics. These are conditional layout-quality
comparisons and do not assign a quality value to the extra H12 completions.
Mean evaluation wall time is `359.21 s` for H6 and `361.39 s` for H12 on
the nine final comparisons, an observed `0.61%` increase.

## Interpretation and setup proposal

The integrated H12 training/search setup materially improves completion and
retention on fresh on-policy-trained models and a reserved held-out panel,
across all three search seeds. For this tested two-facility distribution, the
evidence supports a production setup proposal using the retained
[horizon-12 search configuration](../benchmarks/hierarchical-target-causal/horizon-12-search.json):
128 simulations per move, 16 root candidates, eight anchor candidates,
24 maximum moves, the original policy target, 100 training epochs, and both
lower and outer model training enabled. The H12 configuration differs from
the H6 control only in its search horizon. Its matching outer-checkpoint
search contract is required.

This comparison changes search horizon and the weights learned under that
search together. The primary diagnostic found that H12-trained finals did
not consistently outperform H6-trained finals under the **same** H12 search;
the mean first-root policy target still placed `99.07%` of its mass on
unvisited actions. The held-out result therefore validates the integrated
setup, not a claim that the learner steadily improves or that training alone
causes the gain. The held-out generator limits cases to two facilities and
at most six connections; broader factory coverage needs its own independent
evaluation before a broader production claim.

## Reproduction and closeout

Under `~/Srv/aic-target-causal-20260925` on `ashrimp-arch`, with the
LibTorch runtime from `docs/maintenance/training-server.md` and the parent
study's retained checkpoints and inputs:

```sh
python3 tools/factory-edit-pilot/hierarchical_lower_restored_heldout.py prepare \
  --output artifacts/recovery-lower-restored-v2
python3 tools/factory-edit-pilot/hierarchical_lower_restored_heldout.py run \
  --output artifacts/recovery-lower-restored-v2
python3 tools/factory-edit-pilot/hierarchical_lower_restored_heldout.py summarize \
  --output artifacts/recovery-lower-restored-v2
```

The [cleanup manifest](2026-09-26-hierarchical-lower-restored-heldout-cleanup.json)
records removal of replayable raw evaluations after the compact protocol,
success masks, conditional quality results, and report hashes were retained.
The parent final checkpoints remain for the explicit attribution and recovery
continuation.
