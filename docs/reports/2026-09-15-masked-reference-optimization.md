# Distribute masked node references before indexed backward

The root policy now spreads masked placeholder references across the existing
node bank instead of clamping every missing reference to node zero. The context
mask still replaces these values, and their node gradients remain zero. Valid
references, selected features, graph context and model/optimizer state formats
are unchanged.

## Cause and bounded change

The preceding `2026-09-15-rust-training-profile.md` identified indexed feature
backward as 87.7% of GPU kernel time and hypothesized that missing references
concentrated zero-valued gradients. Retained actual PPO batches confirm the
concentration:

| Actual retained batch | Nodes | Candidate rows | Missing reference entries | Entries mapped to zero before the change |
| --- | ---: | ---: | ---: | ---: |
| `native-collection-final-128/chunk-0000` | 14,824 | 353,632 | 173,976 | 174,184 |
| `native-collection-final-128/chunk-0001` | 14,824 | 314,620 | 171,456 | 171,632 |

The previous `refs.clamp_min(0)` sends every absent reference to one node before
the `where` context mask removes its value. Backward still processes those zeros
through the deterministic repeated-index accumulation kernel.

`masked_references` constructs placeholder indices as flattened reference
ordinal modulo node count. A tensor `where` preserves every valid index and
substitutes these balanced destinations only for negative references. The
existing advanced index operation and context mask remain in the autograd graph.
This removes the concentration without relaxing deterministic algorithms,
changing the summation order of valid references or adding custom CUDA kernels.
No node rows, dummy feature bank, recovery fields or new dependencies are added.

The input remains a nonempty node bank, a two-column integer reference tensor
with valid node indices or negative missing markers, and one context row per
candidate. The output remains `[candidates, 2, width]`; each missing position
contains that candidate's context. Placeholder construction stays batched on
the tensor's device and introduces no host synchronization or per-item work.

## Continuity checks

| Existing benefit/contract | Preservation and evidence |
| --- | --- |
| Exact selected features and masked context | Direct CPU/CUDA comparison with the old expression |
| Node/context gradients and deterministic accumulation | Exact backward comparison for repeated valid indices, no/some/all missing references, one/many nodes and repeated execution |
| Model, Adam and independent RNG continuation | Existing retained actual CUDA minibatch oracle remains exact; existing checkpoint tests run with the changed model |
| Batched policy and one-batch-ahead PPO preparation | Reused unchanged; measured through the actual 128-stream CLI |
| Full PPO/SIL coverage, masks and actor-safe tails | All 64 minibatches per update, four critic epochs, the same one/four actor epochs and replay counts |
| Sparse storage, resource limits and recovery | Existing runtime/checkpoint path; measured publication cost and retained output size |

## Measured cached-input result

Release runs use the same RTX 5090, actual checkpoint 2 and cached next-wave
inputs, 128 streams, 16 workers, width 40, 256-row minibatches and four PPO epochs.
Each arm completes two updates and counts **8,192 new transitions**. Neither
throughput measurement uses a profiler. The before arm uses the saved unmodified
binary from the preceding profile; a fresh matched control avoids mixing older
intermediate runtime/storage revisions into the comparison.

| Metric | Before | After |
| --- | ---: | ---: |
| Complete invocation | 15.131 s | 7.641 s |
| PPO | 10.178 s | 2.793 s |
| Collection | 3.276 s | 3.201 s |
| Input admission | 0.298 s | 0.303 s |
| Checkpoint publication | 0.061 s | 0.056 s |
| New samples/minute | 32,484 | 64,326 |

PPO time falls **72.6%** and bounded overall throughput increases **98.0%**.
The second, fully actor-active update falls from about 7.30 s to 1.44 s of PPO.
Both cached runs select one then four actor epochs, keep all four critic epochs,
and finish with 178 replay sequences and 694 labels. The optimized run retains
29.2 MB of output; peak observed process-tree RSS is 7.245 GB. These are bounded
measurements, not a statistical throughput or learned-quality claim.

The comparison includes restore, collection, complete PPO, replay work and
checkpoint publication. Fresh factory construction and scheduled held-out
evaluation are outside these two cached-input updates. The separate fresh-input
preparation cost identified in the prior report is unaffected by this change.

## Fresh-input end-to-end measurement

A subsequent actual fresh run measured the cost omitted by the cached benchmark.
It uses the same optimized binary, configuration, 128 streams and 16 workers,
but starts from initialization and constructs both factory waves. It completes
four updates and counts **16,384 new transitions**. All 256 minibatches execute;
actor epochs are 4/4/1/4 and critic epochs are 4/4/4/4, matching the retained
pre-optimization fresh run. No profiler is enabled.

| Fresh-input metric | Before, retained run | After |
| --- | ---: | ---: |
| Complete invocation | 47.755 s | **29.045 s** |
| Factory generation/admission | 15.342 s | **15.472 s** |
| Collection | 6.124 s | 6.109 s |
| PPO | 23.945 s | 5.119 s |
| Checkpoint publication | 0.118 s | 0.111 s |
| New samples/minute | 20,585 | **33,846** |

The fresh workflow improves 64.4% in samples/minute over the retained fresh
baseline. Its measured rate is about 53% of the optimized cached-input rate.
The 64,326 samples/minute cached result therefore does not describe ordinary
training with fresh factory generation. Factory generation/admission now takes
53.3% of the measured invocation and is the next overall bottleneck. Its internal
construction/randomization breakdown remains unmeasured.

The run includes initialization, fresh construction, collection, complete PPO,
replay work and normal checkpoint publication; the remaining 2.234 s outside the
listed phases includes initialization and coordinator/replay work. It retains
29.2 MB of output and finishes with 178 replay sequences and 694 labels. The
128-update evaluation interval is not reached in this short run. This is a
bounded fresh-workflow measurement, not a long-run rate including periodic
evaluation or a learned-quality result.

Reproduction uses a new output without cached preparation or `--resume`:

```sh
target/release/aic-cli learning train \
  --config tools/native-training/training.json \
  --output artifacts/rust-masked-reference-opt/fresh-after --updates 4
```

## GPU confirmation

A separate bounded Kineto run with the same two updates confirms the cause:

| Recorded GPU/CPU activity | Before | After |
| --- | ---: | ---: |
| `IndexBackward0 > _index_put_impl_` GPU kernels | 7.712 s | **0.310 s** |
| All GPU kernels | 8.796 s | 1.399 s |
| Forward `index_add_` GPU accumulation | 0.259 s | 0.259 s |
| CPU `cudaStreamSynchronize` | 7.170 s | 0.102 s |

The dominant indexed-backward cost falls **96.0%**. The number of synchronizations
is identical (14,066); the reduced wait confirms that most of the previously
observed CPU stall was waiting for this GPU work. Kernel count rises slightly
from 260,792 to 263,204 because placeholder distribution adds inexpensive tensor
operations. The improvement comes from removing serialized zero accumulation,
not from skipping learning work or checks. CPU waits overlap GPU execution and
must not be added to kernel timings.

## Verification

`cargo fmt --all`, default workspace check/tests and feature-enabled workspace
check/tests passed. The feature-enabled debug test run used
`RUST_MIN_STACK=16777216`: its initial default-stack run hit the existing CLI
parser test's stack overflow (`parses_cumulative_facility_coordinate_partition`).
This is a test-worker setting; normal CLI settings are unchanged.
The focused CPU/CUDA forward/backward tests pass exact tensor
equality, as does the explicit retained actual CUDA model/Adam/RNG regression.
The actual training command and the separate GPU trace both complete the full
configured workload. Documentation/artifact maintenance checks passed.

Compact provenance, timings and kernel attribution are recorded in
`2026-09-15-masked-reference-optimization-evidence.json`. Profilers and diagnostic
processes were stopped after the bounded runs; no long training run was launched.

## Reproduction

Use the existing LibTorch environment and retained actual source:

```sh
cargo build --release -p aic-cli --features learning
node tools/native-training/prepare-cached.mjs \
  artifacts/native-training-fresh-final \
  artifacts/rust-masked-reference-opt/cached-after 2
target/release/aic-cli learning train \
  --config tools/native-training/training.json \
  --output artifacts/rust-masked-reference-opt/cached-after --updates 4 --resume

cargo test --release -p aic-learning --features libtorch model::tests
AIC_RETAINED_COLLECTION="$PWD/artifacts/native-collection-final-128" \
  cargo test --release -p aic-learning --features libtorch \
  retained_collection_updates_remain_exact -- --ignored
```

Output paths must be new. Normal training needs no new option or environment
variable. The old clamp expression exists only in the focused regression test.
Raw actual outputs and verification logs are retained under
`artifacts/rust-masked-reference-opt` on the existing host and summarized locally
under `artifacts/rust-masked-reference-opt-20260915/`.
