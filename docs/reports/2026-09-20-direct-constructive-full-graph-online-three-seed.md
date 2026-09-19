# Direct constructive online full-graph three-seed result

- Date: 2026-09-20
- Source commit: `284ee890752fead4dfea0d1871a13a7e34ee459d`
- Release binary SHA-256: `be839a50c8bd026ef21c676eb7bbc95ae9c474a070491fd38b93103284199336`

## Verdict

Fixed-panel construction reward rose in all three initialization seeds. The
three-seed stochastic mean increased from `-0.475811` before training to
`-0.463551` after 32 updates, an endpoint gain of `+0.012261`. Every seed also
had a positive first-four to last-four evaluation-window change and a positive
least-squares slope. The mean trace itself never decreased.

This does not establish physical-layout learning. The final connection
completion ratio was zero in every seed, no evaluation layout was fully
validated, and the only nonzero completion observation was `0.003125` for seed
202 at update 8 before returning to zero. Because completed connections were
zero at the final point, the retained reward gain came from placing required
facilities and boundary terminals. All three policies converged to the same
final reward. Seeds 101 and 202 stopped improving by updates 20 and 12,
respectively; seed 303 reached that plateau at update 4.

The result therefore verifies that online full-graph PPO changes the policy and
raises its committed-construction reward across seeds. It also exposes the next
learning blocker: the policy quickly learns early placement progress and then
settles on a no-connection behavior. A larger or longer production run should
wait for a bounded diagnosis of that routing and credit-assignment failure.

## Controlled protocol

The retained configuration is
[`config.json`](../benchmarks/direct-constructive-full-graph-online/config.json),
and graph generation uses the established
[`sampling.json`](../benchmarks/direct-constructive-full-graphs/sampling.json).
Each arm used:

| Field | Value |
| --- | ---: |
| Model width / message rounds | 128 / 4 |
| Updates / fresh graphs per update | 32 / 8 |
| PPO epochs / minibatch size | 4 / 128 |
| Episode turn guard | 512 |
| Fixed evaluation graphs / interval | 8 / every 4 updates |
| Learning rate / entropy coefficient | 0.003 / 0.01 |
| Problem seed / evaluation seed | 101 / 700101 |
| Initialization seeds | 101, 202, 303 |

Only `initialization_seed` changed between arms. Each update generated a new
balanced training batch from normalized catalogs. The three arms intentionally
used the same deterministic online graph stream so policy initialization is the
only comparison variable; no serialized training pool was loaded or replayed.

Evaluation used one fixed held-out graph from every populated power-of-two
facility band. In band order, the panel contained 2, 4, 8, 16, 20, 48, 96 and
143 facilities. Its stored SHA-256 was identical in all arms:
`ee6e85cf18b89e70bd8fce29806ae44da283f3d8d8303458a8c224321c9d4a9e`.
The fixed panel makes time points directly comparable while keeping evaluation
targets disjoint from training targets.

The 512-turn guard is a bounded trend experiment, not the 4,096-turn production
configuration. It cannot demonstrate completion of the largest graphs. The
zero completion result also occurred in small held-out bands, so the guard does
not by itself explain the learned no-connection behavior.

## CUDA memory correction

The first seed-101 attempt used minibatch 256. It completed 29 updates and then
failed during update 30 while trying to allocate another 3.58 GiB. PyTorch
reported 28.01 GiB already in use and only 3.39 GiB free on the 31.44 GiB CUDA
device. That interrupted run produced 30 metric rows but no report or learner
snapshot and is excluded from the learning result.

Reducing the minibatch to 128 preserved all 4,096 rollout samples, four PPO
epochs and one optimizer step per epoch. It changed only the number of live
autograd samples: each epoch used 32 minibatches instead of 16. All three
corrected runs completed, including report and checkpoint serialization. Their
largest monitored allocation was 29,758 MiB. Both tracked online and production
full-graph configurations now use 128; the 4,096-turn production configuration
still requires its own end-to-end capacity check.

## Learning result

The primary metric is fixed-seed stochastic evaluation reward. Window means use
updates 0, 4, 8 and 12 for the first window and 20, 24, 28 and 32 for the last.
The regression delta is the fitted per-update slope multiplied by 32.

| Seed | Initial | Final | Endpoint delta | First 4 mean | Last 4 mean | Window delta | Regression delta | Max completion | Wall | Peak VRAM |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 101 | -0.470716 | -0.463551 | +0.007166 | -0.468593 | -0.463551 | +0.005042 | +0.007792 | 0 | 3,029 s | 24,598 MiB |
| 202 | -0.484025 | -0.463551 | +0.020474 | -0.472301 | -0.463551 | +0.008750 | +0.016920 | 0.003125 | 3,040 s | 26,438 MiB |
| 303 | -0.472692 | -0.463551 | +0.009142 | -0.465836 | -0.463551 | +0.002285 | +0.004875 | 0 | 3,080 s | 29,758 MiB |
| Mean | -0.475811 | -0.463551 | +0.012261 | -0.468910 | -0.463551 | +0.005359 | +0.009863 | 0.001042 | 9,149 s total | 29,758 MiB max |

The aggregate stochastic trace was:

| Update | Mean reward | Mean connection completion |
| ---: | ---: | ---: |
| 0 | -0.475811 | 0 |
| 4 | -0.470303 | 0 |
| 8 | -0.465795 | 0.001042 |
| 12 | -0.463730 | 0 |
| 16 | -0.463640 | 0 |
| 20 | -0.463551 | 0 |
| 24 | -0.463551 | 0 |
| 28 | -0.463551 | 0 |
| 32 | -0.463551 | 0 |

Greedy construction reward showed the same early change more strongly: its
three-seed mean rose from `-0.687526` to `-0.463551`. Seed 101 fluctuated after
that jump, while seeds 202 and 303 reached the common final value at update 4
and stayed there. Final greedy and stochastic policies therefore had the same
mean reward and zero completion.

The endpoint reward gain was concentrated in the two largest held-out cases:

| Facility band | Initial mean reward | Final mean reward | Delta | Final completion |
| --- | ---: | ---: | ---: | ---: |
| 1-2 | -0.428571 | -0.428571 | 0 | 0 |
| 3-4 | -0.428571 | -0.428571 | 0 | 0 |
| 5-8 | -0.428571 | -0.428571 | 0 | 0 |
| 9-16 | -0.454545 | -0.454545 | 0 | 0 |
| 17-32 | -0.476190 | -0.476190 | 0 | 0 |
| 33-64 | -0.466667 | -0.466667 | 0 | 0 |
| 65-128 | -0.528736 | -0.482759 | +0.045977 | 0 |
| 129-256 | -0.594636 | -0.542529 | +0.052107 | 0 |

This localization is consistent with learning more placement progress on large
graphs rather than learning a general connection strategy.

## Optimizer behavior

Every arm recorded 128 optimizer steps. PPO updates were active, but seed 303
had two large late epoch-level KL excursions without a corresponding evaluation
change.

| Seed | Mean approximate KL | Maximum approximate KL | Maximum clip fraction |
| ---: | ---: | ---: | ---: |
| 101 | 0.001965 | 0.015629 | 20.12% |
| 202 | 0.001687 | 0.015758 | 24.34% |
| 303 | 0.005987 | 0.214508 | 43.46% |

Seed 303's largest KL value occurred at update 24; another `0.107800` excursion
occurred at update 26. Its fixed evaluation had already been flat since update
4 and did not change after either excursion. The optimizer is therefore not
inactive, but its later changes do not improve the held-out construction policy.

## Online generation coverage

The deterministic graph stream and per-update summaries matched across all
three arms. Within each arm, online generation selected 256 graphs from 65,536
attempts:

| Coverage | Value per arm |
| --- | ---: |
| Selected graphs / globally unique graphs | 256 / 227 |
| Unique target items | 104 |
| Facility-count range / mean | 1-251 / 48.496 |
| Selected graphs per facility band | 32 |
| Accepted candidates | 25,064 |
| Duplicate candidates | 4,454 |
| Facility-guard rejections | 62 |
| Flow failures / source failures | 35,956 / 0 |

The 29 repeated selected graph identities arose from repeated online draws
across updates. They were not read from a prepared pool. Every update still ran
the sampler and produced one selected case from each of the eight facility
bands.

## Evidence and reproduction

Run one arm with the retained inputs, changing only `initialization_seed` for
the other arms:

```sh
aic learning train-constructive \
  --config docs/benchmarks/direct-constructive-full-graph-online/config.json \
  --sampling docs/benchmarks/direct-constructive-full-graphs/sampling.json \
  --catalog-root data/game/normalized \
  --output target/direct-constructive-full-graph-online-seed-101 \
  --device cuda:0
```

The catalog SHA-256 was
`d64fc5916e2d57c3052dd8f7eeefd85b4211ad222553c450391b601d64a51cca`,
and the sampling SHA-256 was
`69c9fe638fc23af815c00ca4698d47d6e448fb15616e1235ef3c3e993a627fc6`.

| Seed | Config SHA-256 | Metrics SHA-256 | Report SHA-256 | Learner SHA-256 |
| ---: | --- | --- | --- | --- |
| 101 | `82d67c5e57269bb477fdc97431c29f5b5494cf3cc6982fa889518139a84c1be2` | `b7950ba1132885f4e0aafacc6b3208c24c2c25e85da8f3f631a5b7c13d0ae620` | `2139a15ebb88d62d3201e1ab4c4d7e1c917d0aab06ff4bd683de77e1f6c86ad8` | `5488334f496da5c8a4099abcc661fcf2fad0544ff7664dc94d1b90a6e36eeade` |
| 202 | `4e92b0f3b3ad8a5b8fc7df89922edcc6a787277906b16b4ea95abb437cfdab49` | `2f8a566130f718d903d6317a9c4a55d7853ec13a60e76c3c547e1dec80065673` | `ee44247a025a83d800b86c4e0a4b5dee4b3716a027d7513865a4b1ebde4239e9` | `b68a9967bf5d7fcd701cfc24b643b8f2e51d39aeb7697d1caf3c29b807f1a752` |
| 303 | `cfebb48b4bc27205b16f08b211bc73989ba74ed7793bafb7341f52392043c138` | `0c142217ac9540c1b77f658dd48cfc878674031a9e4191936ce51c39d3cf055e` | `55cffdce4cdf541e1d989db9076660949082977094b06844d0d37469cc13feda` | `dd0b1e3713294678585f24c197148d5370d6642bdaad4032aa1dc336d0eccc96` |

All reports declare status `completed`, 32 updates, 256 training episodes,
131,072 training turns and 128 optimizer steps. Each metrics file has the
initial evaluation plus all 32 update rows. The common evaluation-file hash,
identical per-update problem-seed sequence and identical graph-scale summaries
confirm the controlled three-initialization comparison.
