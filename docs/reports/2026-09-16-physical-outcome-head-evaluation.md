# Physical outcome head evaluation on factory inputs

## Result

The final online-pretrained predictor transfers useful outcome ranking and
regression signal to actual factory inputs, but its confidence is unreliable on
small factories. Completion is the strongest head: across 979 changed actions
it reached 0.9667 AUROC, 89.89% threshold accuracy and 0.0834 Brier score. The
same-panel constant baseline reached 83.66% accuracy and 0.1367 Brier score.

Pending and conflict classification also rank outcomes well, at 0.9220 and
0.9548 AUROC. Their Brier scores beat the descriptive constant baselines, but
their log losses do not. A few confidently wrong small-factory predictions
dominate the penalty. The largest completion misses assigned probabilities from
0.00075 to 0.00164 to actions that actually completed. This is a calibration and
small-factory transfer weakness rather than an absence of learned ordering.

All five informative regression heads beat an evaluation-fitted constant on the
combined panel. Count errors averaged 3.68 pending routes and 0.63 conflicts.
For completed layouts, errors averaged 76.42 area cells, 11.40 transport cells
and 2.64 turns. These raw errors average cases with different initial sizes; the
normalized errors are the comparable model targets.

## Fixed measurement

- Predictor checkpoint: update 67,483 from the completed eight-hour online run,
  SHA-256 `ac9ff8acfadd1ca8e76c49cc437d507bddd828b0c50b7380cb637ed121632377`.
- Panel: 16 immutable actual factory inputs, balanced between small and large
  strata, SHA-256
  `d3b33ee8251b90e9a6574c122f4ec82fedb40530b0ee0333d84167262ec8e637`.
- Sampling: two fixed-seed replicates per case and 32 family-balanced random
  compatible actions per replicate.
- Coverage: all 32 episodes completed, producing 1,024 charged transitions.
- Labels: the exact masks and target transforms used in online pretraining.
  Post-action pending and conflict counts came from the next authoritative
  factory observation. Validated layout scores came from native acceptance.
- Runtime: 1.07 seconds on CUDA after the release binary was built.

The constant columns below are fitted to the evaluation labels themselves. They
are descriptive difficulty references, not independently estimated models.

| Binary head | Labels | Positive | Accuracy | Constant accuracy | Brier | Constant Brier | AUROC |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Completion | 979 | 160 | 89.89% | 83.66% | 0.0834 | 0.1367 | 0.9667 |
| Changed | 1,024 | 979 | 94.73% | 95.61% | 0.0441 | 0.0420 | 0.8776 |
| Pending flag | 979 | 778 | 86.72% | 79.47% | 0.1153 | 0.1632 | 0.9220 |
| Conflict flag | 979 | 654 | 83.25% | 66.80% | 0.1356 | 0.2218 | 0.9548 |
| Unchanged | 1,024 | 45 | 94.63% | 95.61% | 0.0448 | 0.0420 | 0.8771 |
| Rejected | 1,024 | 0 | 100% | 100% | 0.0000 | 0.0000 | unavailable |
| Route budget | 979 | 0 | 100% | 100% | 0.0000 | 0.0000 | unavailable |

Rejected and route-budget accuracy is vacuous because the valid-action sampler
produced no positive examples. This run therefore measures discrimination for
five binary heads and only negative-case behavior for those two heads.

| Regression head | Labels | Normalized MAE | Constant MAE | Raw MAE |
| --- | ---: | ---: | ---: | ---: |
| Pending count | 979 | 0.1067 | 0.2549 | 3.68 routes |
| Conflict count | 979 | 0.0690 | 0.1463 | 0.63 conflicts |
| Area | 160 | 0.0933 | 0.1450 | 76.42 cells |
| Transport | 160 | 0.1434 | 0.1820 | 11.40 cells |
| Turns | 160 | 0.3519 | 0.5027 | 2.64 turns |

## Size split

Completion generalizes much better to the large stratum. Large factories reach
0.9975 AUROC, 98.02% accuracy and 0.0132 Brier score. Small factories reach
0.9399 AUROC, 81.22% accuracy and 0.1581 Brier score. The small-factory
completion log loss is 0.6455, worse than its fitted constant at 0.5288, while
the large-factory log loss is 0.0505 versus 0.3442. Every recorded worst
completion error occurs on a small copper-liquid case.

The count split shows the same boundary. On large factories, pending and
conflict normalized MAE are 0.0432 and 0.0273. On small factories they rise to
0.1743 and 0.1135; small-factory conflict count is worse than its fitted
constant. Geometry errors are also concentrated in small watered-grass cases,
although raw area error is larger on large layouts because each normalized unit
represents more cells.

## Scope

This evaluates the pretrained encoder together with the disposable supervised
heads that trained it. It directly answers whether the representation can
predict its training outcomes on real factory observations, but it is not a
head-free probe of the encoder and does not measure PPO policy quality. The
earlier paired PPO screen remains the downstream behavior test.

The reusable command is `learning evaluate-outcome-heads`; its registered
configuration is
[`physical-outcome-head-evaluation.json`](../../tools/native-training/physical-outcome-head-evaluation.json).
The complete per-head, per-stratum, per-product and largest-error evidence is in
[`2026-09-16-physical-outcome-head-evaluation-evidence.json`](2026-09-16-physical-outcome-head-evaluation-evidence.json).
