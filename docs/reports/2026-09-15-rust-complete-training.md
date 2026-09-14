# Complete Rust training and bounded throughput

The native CLI now trains from fresh initialization through full PPO schedules,
new factory waves, causal success replay, best-start curriculum, held-out
evaluation and checkpoint continuation. The temporary `collect` command and
external seed-import runtime have been deleted. Existing Python source remains
unchanged. Commands and migration are in `tools/native-training/README.md`;
contracts are in `docs/designs/2026-09-15-rust-complete-training.md`.

## Measured work

Release executions used the existing Linux CUDA host, 128 logical streams,
16 persistent workers, width 40, two message rounds, 32-action chunks,
64-action episodes, 256-row minibatches, four PPO epochs and target KL 0.02.
Every update processed 4,096 new transitions and **64 minibatches**. Replay and
repeated epoch uses are not new samples. Each measured arm uses its normal
optimizer, replay and checkpoint path. No training traces or profiling were on.

| Measurement | New samples | Seconds | Samples/minute |
| --- | ---: | ---: | ---: |
| Rust, four updates with fresh factory construction | 16,384 | 47.755 | 20,585 |
| Rust, two updates from actual cached next-wave inputs | 8,192 | 15.090 | 32,573 |
| Historical Python, two updates with cached inputs | 8,192 | 20.202 | 24,330 |

The cached Rust measurement is approximately **33.9% higher throughput** than the
retained Python result. Its timer includes checkpoint restoration and initial
CUDA work, while the historical Python metric starts after warmup. Inputs differ
between the languages, as permitted for this comparison. Rust selected one and
four actor epochs; Python selected one and two. Both ran all four critic epochs
and the complete configured rollout coverage. These are bounded individual runs,
not a statistical speedup or learned-quality claim.

For the cached Rust arm, collection took 3.278 s, full optimization 10.141 s,
actual-input admission 0.295 s and checkpoint publication 0.052 s. The remaining
1.324 s includes initialization/restore, replay admission and other coordinator
work. Peak observed process-tree RSS was 7.947 GB. Evaluation was not scheduled
inside these updates; the historical comparison likewise excludes final panel
work. Separate smoke executions exercised actual held-out evaluation.

The fresh arm spent 15.342 s constructing and admitting two factory waves.
It therefore must not be compared directly with the historical cached-input
throughput. The first wave produced 84 admitted successful sequences. The second
ended with 178 sequences and 694 retained labels. The next-start plan included
13 depth-one and three depth-two donors, with shortages replaced by fresh starts.
Twelve inherited starts were actually consumed in the second wave.

## Persistence and continuity

A first integrated measurement exposed unnecessary intermediate input storage:
writing all active preparation inputs after every chunk retained about 660 MB
for four updates. Final storage saves newly retained inputs at the configured
checkpoint cadence. Default complete-wave publication drops unsuccessful
preparation before disk I/O. The same four-update workflow now retains **29.2 MB**,
with 22.7 MB of hydrated successful numerical inputs. Checkpoint work fell from
1.142 s to 0.118 s. Sparse replay archives remain immutable and shared by retained
recovery checkpoints. A requested mid-episode save still preserves its actual
active segments.

The final smoke check saved after update 1 (32 actions), exited, and resumed in a
separate process with one worker instead of two. It completed update 4, evaluated
a separate balanced four-case panel, and saved the best model inside the atomic
checkpoint. Another invocation resumed from update 4 with a target of 20; the
`learning stop` command caused a valid save and exit at update 6, the next complete
wave. Both evaluation strata were also exercised in a newly prepared 32-case
panel, with 16 unique actual geometries per stratum.

Focused tests cover complete PPO coverage including a short final minibatch,
actor/Adam preservation through critic-only tails, replay preparation across
chunk boundaries and terminal returns, causal admission, semantic deduplication,
stage-normalized weights, observed-utility eviction, independent evaluation RNG,
integer quota rotation, strict donor improvement, root-depth identity, reserved
geometry and preservation of interrupted outputs. The CPU and CUDA learner
checkpoint tests restore every saved field and produce an identical next update.
The retained actual CUDA minibatch regression also remains exact.

Whole historical trajectories are not promised to be byte-identical across
processes. The optional full-trajectory comparison was not byte-identical; it is
not used as evidence of exact replay. Required checkpoint field restoration,
logical episode ownership and immediate numerical continuation are tested
directly without imposing historical trajectory replay on normal recovery.

## Verification and reproduction

`cargo fmt --all`, `cargo check --workspace`, the default workspace tests and
workspace check/tests with `aic-cli/learning` passed. The final feature-enabled
workspace run passed 569 tests with five intentionally ignored tests. The ten
focused training tests were subsequently rerun after adding cold checkpoint
identity validation, including its new rejection test, and passed. The retained
CUDA numerical oracle was run separately and passed. Documentation/artifact
maintenance validation passed. No long training experiment was started.

Actual reports, update records and verification logs are retained locally under
`artifacts/rust-complete-training-20260915/`. The compact tracked evidence manifest
is `docs/reports/2026-09-15-rust-complete-training-evidence.json` and records the
binary, source-tree/configuration identity, runtime outputs and comparison limits.
The server outputs are under `/home/ashrimp/Srv/aic-rust-learning-20260915/artifacts`.

To repeat the final measurement using the existing LibTorch environment:

```sh
target/release/aic-cli learning train \
  --config tools/native-training/training.json \
  --output artifacts/native-training-fresh --updates 4

node tools/native-training/prepare-cached.mjs \
  artifacts/native-training-fresh artifacts/native-training-cached 2

target/release/aic-cli learning train \
  --config tools/native-training/training.json \
  --output artifacts/native-training-cached --updates 4 --resume
```

Prepare the held-out panel first as documented in the native-training README.
The cached helper copies the actual second-wave slots and checkpoint 2 into a
new output, before timing. Normal restore validates their source and geometry
reservation; no historical input is relabeled or synthesized. Each output path
must be new, and cached preparation requires the retained completed four-update
fresh output.
