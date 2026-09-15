# Rust training profile: indexed feature backward dominates

The largest remaining cost is LibTorch's backward accumulation for gathered
node features. Rust removed coordinator/collection overhead, but this GPU work
still uses the same eager ATen algorithms. Optimizing general Rust dispatch,
Adam, checkpoint I/O or matrix multiplication first would miss the dominant
measured cost.

## Scope and retained baseline

Profiled the existing release binary from `625f3e2` on the same RTX 5090 host,
using 128 streams, 16 persistent workers, width 40, two message rounds, 256-row
minibatches and four complete PPO epochs. Actual checkpoint 2 and its next-wave
inputs came from the retained fresh run. No training source, numerical setting,
deterministic policy, replay rule or checkpoint behavior was changed.

| Two cached-input updates, 8,192 new samples | Unprofiled baseline | Kineto run |
| --- | ---: | ---: |
| Total training time | 15.090 s | 15.955 s |
| PPO | 10.141 s | 10.612 s |
| Collection | 3.278 s | 3.579 s |
| Input admission | 0.295 s | 0.288 s |
| Checkpoint publication | 0.052 s | 0.064 s |

Both processed 64 minibatches per update, with one then four actor epochs and
four critic epochs throughout. Profiling added about 5.7% to this bounded total;
the recorded phases exclude trace export after training returns. Do not report
profiled timings as normal throughput. The earlier baseline is retained in
`2026-09-15-rust-complete-training.md`, not rerun unnecessarily.

## Kernel evidence

The CPU/CUDA trace contained 260,792 kernels with 8.796 s of GPU kernel activity.
CPU, CUDA API and GPU durations overlap and cannot be added.

| GPU work, attributed through recorded ATen ancestry | Kernel time |
| --- | ---: |
| `IndexBackward0 > aten::_index_put_impl_` | **7.712 s** |
| Forward `index_add_` through indexed accumulation | 0.259 s |
| `aten::index` forward gathers | 0.155 s |
| `aten::addmm` plus `aten::mm` | 0.146 s |

Indexed gather backward accounts for **87.7% of kernel time**. The single
`indexing_backward_kernel<float, 4>` kernel family took 7.656 s; its name alone
does not identify the caller, so the table uses CPU event ancestry to distinguish
actual `IndexBackward0` from forward `index_add` and other backward operations.

There were 14,066 `cudaStreamSynchronize` calls totaling 7.170 s on the CPU, and
`aten::_local_scalar_dense` accumulated 7.093 s. Much of this is waiting for
already queued GPU work, not an additional seven seconds of independent CPU
computation. Copies took 0.335 s on the device; `cudaLaunchKernel` occupied
0.706 s on the host. Although 97.3% of kernels lasted under 10 microseconds, the
few expensive indexing kernels dominate duration. Launch fusion may help later,
but kernel count alone gives the wrong priority here.

GPU kernel/memory activity occupied 9.142 s of its 15.716 s first-to-last activity
span. This is a timeline interval union, **not** SM occupancy or hardware compute
utilization. CPU minibatch preparation totaled 1.370 worker-seconds and overlaps
device work through the existing one-batch-ahead supplier; it is not an additive
critical-path cost.

## Shape attribution

A second diagnostic resumed an actual mid-wave checkpoint 3 and recorded shapes
for update 4 only. It still ran all 64 minibatches and four actor/critic epochs;
PPO took 7.905 s. Shape recording can affect allocation/lifetime behavior, so this
arm is used for attribution, not a new speed comparison.

Indexed backward consumed 6.105 s of that arm's 6.719 s of kernel activity.
Of the 6.105 s, **5.882 s (96.3%)** handled rank-three incoming gradients: the
`[candidate_count, 2, 40]` features gathered from two referenced nodes per
candidate. Two-dimensional incoming gradients cost 0.168 s and scalar-vector
gathers cost 0.055 s.

One expensive observed shape scattered an incoming `[342564, 2, 40]` gradient
back into `[17137, 40]` node gradients. Its associated kernels took 104.55 ms.
That incoming FP32 tensor alone represents about 109.6 MB of expanded features.
This identifies the two-node feature gathers in `Model::roots` and `Model::ports`
in `crates/aic-learning/src/model.rs` as the first optimization target. The trace
does not tag these two Rust call sites separately, so their individual shares
are not claimed.

The installed Torch source revision is
`cf30153c4c131c8164ee7798e5022d810682e2cb`. Its matching
[CUDA indexing implementation](https://github.com/pytorch/pytorch/blob/cf30153c4c131c8164ee7798e5022d810682e2cb/aten/src/ATen/native/cuda/Indexing.cu#L235-L303)
handles each run of repeated destination indices in a loop. The observed
gathered-feature expansion and this accumulation behavior explain why a small
width-40 model can still spend seconds in indexing rather than matrix math.
This is a source-supported interpretation of the measured kernel attribution.

An additional code-level hypothesis is that missing root references, clamped
to global node zero before masking, concentrate even zero-valued gradients at
one destination. This has **not** been isolated by a controlled experiment; it
should be checked before selecting a replacement algorithm.

## Priorities and limits

1. Optimize two-node candidate-feature gathering and its backward accumulation,
   preserving masks, missing-reference behavior, deterministic execution and
   numerical learning contracts. Compare real integrated PPO after any change;
   a faster isolated gather is not sufficient evidence.
2. Address fresh factory preparation separately. The retained fresh four-update
   run spent 15.342 of 47.755 s constructing/admitting two waves. This profile used
   actual cached inputs and does not identify the constructor/randomization
   substage responsible for that cost.
3. Revisit scalar synchronization, repeated epoch-statistics forward work and
   small-kernel launch overhead after the dominant indexing cost is reduced.
   Checkpoint publication is already small in the measured workflow.

No optimization, determinism relaxation or long training run was performed.
Held-out evaluation was not scheduled in these two updates. CPU native workers
and preparation threads are not fully captured by main-thread ATen callbacks;
their existing phase timers provide context rather than a CPU flamegraph.

## Reproduction and verification

`tools/native-training/diagnostics/README.md` contains the exact build, cached
preparation, bounded invocation and offline analysis commands. The diagnostic
preload wraps the existing executable and exports Kineto traces at exit; normal
training has no new hooks. The summary script uses only Python's standard library
offline and is not part of the Rust learner runtime.

The preload compiled against the installed LibTorch and both actual CUDA traces
completed successfully. `cargo check --workspace`, `cargo test --workspace` and
documentation/artifact maintenance validation passed. No production Rust source
was edited. The compact evidence manifest is
`2026-09-15-rust-training-profile-evidence.json`; summaries and run outputs are
retained locally under `artifacts/rust-training-profile-20260915/`. Raw traces and
actual diagnostic checkpoints remain on the host under
`/home/ashrimp/Srv/aic-rust-learning-20260915/artifacts/rust-training-profile/`.
