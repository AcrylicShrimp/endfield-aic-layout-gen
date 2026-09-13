# Bounded replay-sampler verification

Explicit sampling settings are implemented and verified for uniform, score-mixture
and positive-advantage reference sampling at 8, 16 and 32 sequences per minibatch.
The main learner remains paused on its original configuration. These mechanism
checks do not select a training setup or establish improved evaluation returns.

## Preserved behavior and intentional differences

Missing settings preserve the original eight-draw implementation and RNG order.
Explicit uniform eight draws also matches it exactly. Every mode retains available
stratum balance, replacement and all labels in each sequence. The loss still
averages over sequences and stages, using true terminal return and positive
return-minus-value. Increasing count changes coverage and cost, not its scale.

Score mixture has a half-uniform probability floor and bounded score mass. It does
not multiply the loss by the score or apply inverse-probability correction. The
positive-advantage reference uses alpha 0.6, epsilon 1e-6, and normalized partial
importance correction with beta 0.1. Raw utility and priority observations remain
uncorrected. This is a sequence-based PPO auxiliary reference, not a reproduction
of SIL's separate optimizer/value-loss procedure.

Distributions are constructed once per update. Sampled priorities refresh from
existing measurements, with no extra inference; warm-cache access reads current
entry metadata before the next distribution is built. Cold entries use the current
stratum maximum, maintained by a tokenized heap with eviction support. Features
are unnecessary for uniform and positive-advantage modes under v1 retention.

## Checks

- Eleven focused sampling tests pass: seeded frequencies and exact probabilities,
  bounds and all-zero priorities, every count/balance mode, loss normalization,
  uncorrected observations, priority bootstrap/removal, invalid inputs, default
  equivalence, no extra priority-refresh forward passes, and KL actor-tail safety.
- For all nine settings, warm priority refresh and checkpoint restoration produce
  identical next-update IDs and complete learner checkpoint state.
- On the server, the old active-source buffer/index/**PPO kernel** and new code
  processed the same two recorded native-fixture updates with nonempty Adam state,
  actual replay and eviction. Model, both optimizers, complete buffer/checkpoint,
  selected IDs, replay/order RNG hashes, ordered events and weighted loss match.
- Actual CLI validates all three sampling kinds; count seven exits 2 with the
  nested stable diagnostic `declarative.replay-sampling.config`.
- Combined declarative suite: 58 tests pass with one platform skip. Execution
  suites: 16 tests pass with five platform/CUDA skips. Workspace check/tests and
  repository hygiene pass. No native or Cargo behavior changed.

## Bounded server cost

Five fixed repetitions use 4,096 entries and 64 minibatch draw calls per update.
Times are medians in milliseconds. This measures the sampler only, with zero
source loads and zero model forwards; integrated learning/storage cost is a
separate readiness gate.

| Kind | Count | Build once | 64 draw calls |
| --- | ---: | ---: | ---: |
| uniform | 8 | 0.708 | 0.228 |
| uniform | 16 | 0.676 | 0.267 |
| uniform | 32 | 0.673 | 0.348 |
| score_mixture | 8 | 3.014 | 0.323 |
| score_mixture | 16 | 2.999 | 0.391 |
| score_mixture | 32 | 3.048 | 0.518 |
| positive_advantage | 8 | 2.120 | 0.362 |
| positive_advantage | 16 | 2.103 | 0.469 |
| positive_advantage | 32 | 2.110 | 0.660 |

## Reproduction

Server source: `/home/ashrimp/Srv/aic-retention-candidates-20260914`. Use the existing
Python environment and unchanged native library:

```sh
AIC_NATIVE_LIBRARY=/home/ashrimp/Srv/aic-declarative-batch-20260913/target/release/libaic_pilot_native.so \
  python tools/factory-edit-pilot/diagnostics/declarative_retention_verify.py \
  --component sampling \
  --config /home/ashrimp/Srv/aic-retention-experiments-20260914/artifacts/retention-verification/v1.json \
  --baseline-runtime /home/ashrimp/Srv/aic-declarative-storage-20260914/tools/factory-edit-pilot \
  --output artifacts/sampling-verification.json
```

Exact output is retained on that server and locally at
`artifacts/retention-experiments/20260914/sampling-verification.json`.
Default configuration digest: `d469865acf7357a6ec59f64b59a5e9a3bef204e0e26c40998eb36477bfd8a45c`.
Exact paired-record digest: `8cb62ee25b81c31aa727f4637a74459e6167fc912c9db247db3496eee190031b`.
