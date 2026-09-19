# Direct constructive full-graph readiness

Date: 2026-09-20
Source commit: `3c8b8a6e7676722021696c43f5179dd06e3a016e`

## Verdict

The full-production-graph cutover builds with the Rust `learning` feature and
completes a bounded CUDA training cycle. The run admitted all twelve prepared
problems, performed policy inference on evaluation graphs containing up to 143
facilities, collected training transitions from a 156-facility graph, completed
one PPO optimizer step and wrote the model and training report.

This is an integration-readiness check. Its two-turn episode limit deliberately
does not measure layout completion or policy quality.

## Feature-enabled build

The isolated source snapshot was built in a preconfigured LibTorch/CUDA
environment with:

```sh
cargo build --release -p aic-cli --features learning --bin aic-cli
```

The release build completed successfully in 49.61 seconds. The emitted warnings
were the existing unused-item warnings in `aic-pilot-native`; there were no build
errors from the physical schema v2, learning schema v2 or problem-set schema v2
cutovers.

## Full-graph preparation

The tracked sampling policy was exercised with seed 101:

```sh
target/release/aic-cli learning prepare-constructive-problems \
  --sampling docs/benchmarks/direct-constructive-full-graphs/sampling.json \
  --catalog-root data/game/normalized \
  --output artifacts/full-graph-smoke/problems.json \
  --seed 101 \
  --training 8 \
  --evaluation 4
```

Preparation completed without creating a constructive session or computing a
physical layout. It selected one training problem from every populated
power-of-two facility-count band and four held-out problems:

| Split | Facility counts in stored order | Covered bands |
| --- | --- | --- |
| Training | 2, 156, 32, 4, 48, 8, 70, 16 | 1-2 through 129-256 |
| Evaluation | 2, 143, 20, 4 | 1-2, 3-4, 17-32, 129-256 |

The preparation report recorded 800 accepted training candidates and 398
accepted evaluation candidates from 2,048 attempts per split. Three training
candidates and one evaluation candidate exceeded the configured 256-facility
sampling work guard. The resulting problem-set SHA-256 was
`359653fe01fcc992f3d5648077c2848377e60c612fa67d4bf768fa07d9fce73c`.

## CUDA integration smoke

The smoke configuration started from the tracked full-graph configuration and
changed only these bounded-execution fields:

| Field | Value |
| --- | ---: |
| `native.max_turns` | 2 |
| `updates` | 1 |
| `episodes_per_update` | 2 |
| `ppo_epochs` | 1 |
| `ppo_minibatch_size` | 32 |
| `evaluation_episodes` | 4 |

The run used `--device cuda:0`. Because training consumes problems in stored
order, its two rollout sessions used the 2-facility and 156-facility graphs. The
initial and final evaluations each traversed all four held-out graphs, including
the 143-facility case.

The command completed with exit code zero and reported:

| Result | Value |
| --- | ---: |
| Device | `cuda:0` |
| Model parameters | 521,346 |
| Training episodes / turns | 2 / 4 |
| Optimizer steps | 1 |
| Final greedy episodes / accepted turns | 4 / 8 |
| Final stochastic episodes / accepted turns | 4 / 8 |
| Dead ends | 0 |
| Written checkpoint SHA-256 | `e7630872f0eec8b1639c8c9a8d7ce967fde49e7a90345910b57e9e7c4aa81bfd` |

The output contained the resolved configuration, immutable problem set,
parameter order, metrics, `learner.safetensors` and the final report. The
reported configuration SHA-256 was
`6541d6b8631bf94197eb44de438042fac4300feb04b1de542acb644720d7488d`;
the loaded catalog SHA-256 was
`d64fc5916e2d57c3052dd8f7eeefd85b4211ad222553c450391b601d64a51cca`.

No episode could finish a full graph within two turns, so all completion ratios
and `fully_validated` counts were zero by construction. Those values are not a
failure signal for this check. The evidence establishes feature-enabled build,
problem admission, large-graph observation and policy inference, CUDA rollout,
GAE/PPO update, checkpoint serialization and report generation. A longer run is
required to evaluate learning effectiveness and complete-layout success.
