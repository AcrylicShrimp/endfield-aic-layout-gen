# PPO padded CNN batches and spatial indexing

The learner now pads each actor/critic minibatch to its maximum height and width,
encodes its factories together with an explicit valid-region mask, and gathers
node/action/member features through CPU-prepared spatial addresses. The previous
implementation already batched 64 selections and combined independent graphs;
this change removes equal-shape CNN fragmentation and per-factory GPU spatial
indexing. It does not change the PPO minibatch size or combine factory graphs.

The user-stopped twelve-hour experiment remains stopped. All work below uses
bounded diagnostics on an RTX 5090, PyTorch 2.14.0+cu130, deterministic FP32,
width-32 two-layer CNNs, four PPO epochs and unchanged curriculum/reward settings.
The frozen baseline is `d22474b` under
`/home/ashrimp/Srv/aic-ppo-data-supply-20260911-r2`. The new runtime is
`/home/ashrimp/Srv/aic-ppo-batched-indexing-20260911`.

## Implementation and mask semantics

Preparation produces zero-padded 28-channel rasters and a separate one-channel
validity mask. GroupNorm computes each factory/group's mean and biased variance
over its valid pixels and channels only. Mask every spatial layer's output,
including convolution biases and residual blocks, so padded activations cannot
enter valid boundary pixels through the next convolution. Spatial pooling divides
by actual valid area. Existing coordinate channels still describe each original
factory's extent. The maximum dimensions come from the current minibatch, not a
fixed game canvas or a feasibility bound.

Spatial addresses encode both factory slot and pixel offset. Batched gathers
restore the original node, candidate and member order, including duplicate
coordinates, absent actor worlds and empty member sets. Graph offsets, message
normalization, candidate masks, current/best aliases and action ordering remain
independent. Obsolete per-world GPU coordinate references are removed.

The internal learner execution schema changes from
`ppo-batched-input-contracts-v2` to `ppo-batched-input-contracts-v3`. Model input
channels, parameter structure and collector observations remain unchanged.
Frozen runs use their original source runtime; this cutover does not promise
bit-exact continuation through new sources.

## Measurements

The retained batch-3 workload has 61 factories, 1,952 selected actions, nonempty
Adam and both inherited start depths. Restore models, optimizers and RNG freshly
for each comparison cell. Explicitly isolate baseline world/batch/supply/update
modules so current imports cannot accidentally alter the baseline.

Resident measurements prepare one fixed shuffled 64-selection minibatch and time
training without input construction or transfer. Three warmed measurements gave
58.63 ms before and 40.55 ms after, 30.8% less time. An intermediate candidate
that grouped spatial gathers while retaining exact-shape CNN buckets took
53.46 ms against its own 58.28 ms baseline. Its source snapshot and failed strict
continuation check remain diagnostic evidence.

Padding increases actor pixels from 72,367 to 535,808 (7.40x) and critic pixels
from 102,783 to 803,712 (7.82x) in that minibatch. Peak allocated CUDA memory rose
from 723,633,664 to 2,165,014,016 bytes. This is a speed/memory tradeoff; larger
outliers can increase padding further. A one-shot preparation measurement was
39.83 ms before versus 56.93 ms after and is not a warmed throughput estimate.

Full updates used one warmup and three alternating measured repetitions. The
baseline took 12.613, 12.562 and 12.583 seconds; padding took 10.710, 10.682 and
10.700 seconds. Median time falls from **12.583 to 10.700 seconds**, 15.0% less
time, including CPU preparation, transfers and epoch statistics. Padded full
updates peaked at 2,595,498,496 allocated CUDA bytes. These isolated timings
exclude collection and supervisor/observer work.

An independent profile reproduced resident timings of 58.90 versus 40.88 ms
outside instrumentation. Instrumented CUDA launches fell from 13,876 to 2,666
(80.8% fewer); CPU time in launch calls fell from 31.77 to 6.43 ms. Convolution
forward/backward operator counts fell from 204 to four each. Padding increases
the work in those larger convolutions and adds explicit masked statistics.
Deterministic indexing backward still accounts for about 14.10 ms of sampled
device time, versus 16.91 ms before. Its nested small-stride kernel time must
not be added again. Indexing and padded memory/transfer remain follow-up costs;
the result does not establish full GPU saturation.

The complete native schedule (four 64-factory attempts, 32-step horizon, two
32-factory evaluations, sixteen collectors) took **92.84 seconds**, compared with
106.09 seconds for the previous idle baseline: 12.5% less time. Both runs use the
same configuration, fixed initial-factory manifest and retention `[1, 3, 4]`.
Process-tree peak RSS was 24,675,983,360 bytes (22.98 GiB), versus 22.94 GiB before,
under the unchanged 32 GiB limit. RSS includes shared pages counted per process.

| Training batch | Admitted factories | Old update seconds | Padded update seconds |
| ---: | ---: | ---: | ---: |
| 1 | 64 | 19.24 | 15.99 |
| 2 | 63 | 18.08 | 14.89 |
| 3 | 61 | 15.99 | 12.95 |
| 4 | 62 | 17.02 | 13.16 |

All training attempts preserve status, initial/best geometry metrics, best
geometry hashes, selected counts, action-kind/status counts and improvement
counts. Every batch preserves optimizer-step count, KL stop decision and shuffle
state; model/Adam hashes and epoch metrics differ. Initial evaluation matches.
Final evaluation changes seven of 32 best geometries/metrics and nine action
kind/status count summaries. Summed best area is 39,199 versus 39,840 before;
one seed and four batches cannot establish a learning-quality improvement.
End-to-end timing therefore measures equivalent configured work, not identical
final evaluation trajectories.

## Numerical differences and rejected strict checks

Before optimizer continuation, frozen/current predictions differ by at most
9.54e-7 and clipped parameter gradients by 4.20e-7. Existing forward tolerances
(`rtol=1e-5`, `atol=1e-6`) and gradient tolerances (`rtol=5e-5`, `atol=5e-6`) pass.
Independent tests compare valid spatial outputs and every parameter gradient
against unpadded encoding, including dilated residual layers and a larger
companion factory. Masked-out activations are zero.

The original strict full-update numerical comparison fails. After 124 Adam
steps, the largest parameter discrepancy is 0.003344 in `actor.score.2.bias`.
This scalar adds the same offset to every action logit; that offset cancels
mathematically in softmax, although finite-precision gradients and Adam can move
it. The next-largest parameter discrepancy is 1.72e-5; critic parameters differ
by at most 3.02e-7. Do not describe every difference as an irrelevant bias.

Across all 1,952 selections after optimization, maximum absolute differences are
6.73e-5 for candidate probabilities, 7.41e-4 for selected log probabilities,
2.29e-4 for entropy and 1.79e-7 for critic values. PPO clipping diagnostics can
change discretely: the largest epoch-field difference is 0.001025, corresponding
to two selections out of 1,952. These are measured differences, not a statistical
claim of unchanged long-run learning or identical future sampled trajectories.

`benchmark_ppo_indexing.py --allow-roundoff` explicitly records strict comparison
failures while permitting numerical measurement. It still requires the old
baseline to match historical hashes exactly, repeated executions of each runtime
to match themselves exactly, and selected counts, optimizer-step counts, KL stop
decisions and shuffle state to match. It does not alter PPO or loosen factory
validation. Original failed strict reports remain in ignored artifacts.

## Verification and retained evidence

The native audit checked 256 retained/evaluation result records and 64 transient
attempts without experience archives. All 32 inherited inputs preserve valid
consecutive ancestry and deterministic start plans. Collector behavior log
probabilities and critic values match the padded learner within existing audit
tolerances. Reconstructing batch 3 from the new run's nonempty Adam checkpoint
matches actor/critic, both Adam states, shuffle state and epoch diagnostics
bit-for-bit. This checks the new runtime's own continuation, not old/new equality.

All 79 PPO tests passed on the native/CUDA server without skips. Local macOS ran
79 tests with three platform-specific skips. `cargo check --workspace`,
`cargo test --workspace`, Git whitespace checks and artifact/size policy checks
passed. No Rust code changed. Frozen baseline/new protocol source hashes verify.
The stopped long-run checkpoint still has SHA-256
`2252e4b979178b1c0ebf5995672256d3b8ea50d3682d9929e8f944ac2db582c5`.

The [compact provenance](2026-09-11-ppo-batched-indexing-provenance.json) identifies
retained reports, the frozen protocols and source snapshots, including the
intermediate grouped-only candidate. Source scripts and this report are tracked;
raw generated evidence and checkpoints remain ignored. Regenerable profiler
traces are removed after retaining the summarized kernel evidence.

## Reproduction

```sh
python tools/factory-edit-pilot/benchmark_ppo_indexing.py --run "$REFERENCE_RUN" --baseline-runtime "$FROZEN_RUNTIME" --output "$FRESH_OUTPUT/verify" --phase verify --repeats 3
python tools/factory-edit-pilot/benchmark_ppo_indexing.py --run "$REFERENCE_RUN" --baseline-runtime "$FROZEN_RUNTIME" --output "$FRESH_OUTPUT/update" --phase update --repeats 3 --allow-roundoff
python tools/factory-edit-pilot/benchmark_ppo_indexing.py --run "$REFERENCE_RUN" --baseline-runtime "$FROZEN_RUNTIME" --output "$FRESH_OUTPUT/kernels" --phase kernels --repeats 3
python tools/factory-edit-pilot/ppo_harness.py prepare --config tools/factory-edit-pilot/ppo-data-supply-readiness.json --output "$FRESH_RUN"
python tools/factory-edit-pilot/ppo_harness.py run --run "$FRESH_RUN"
python tools/factory-edit-pilot/check_ppo_harness_native.py --run "$FRESH_RUN" --output "$FRESH_OUTPUT/native-audit"
python tools/factory-edit-pilot/compare_ppo_indexing_runs.py --before "$REFERENCE_RUN" --after "$FRESH_RUN" --output "$FRESH_OUTPUT/native-comparison.json"
```

Use the recorded server environment and the reference's
`artifacts/factory-edit-pilot/data-supply-readiness-16` run. Generated profiles,
traces, checkpoints and source snapshots remain under ignored artifacts.
