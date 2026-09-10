# PPO data-supply optimization

The same four 64-factory batches and two evaluations took **192.16 seconds with
the frozen implementation and 115.20 seconds with native-v4**, using eight
collectors in both runs: 40.0% less wall time, or 1.67x throughput. Every batch's
actor, critic, both Adam states, shuffle state and epoch diagnostics matched
exactly. The user-stopped twelve-hour experiment remains stopped at 32 batches /
64,608 admitted actions; these measurements are bounded readiness diagnostics.

With sixteen collectors, the same execution took **106.09 seconds**, 44.8% less
than the original eight-collector baseline (1.81x throughput), again with exact
results. The future-run curriculum configuration now selects sixteen workers;
the stopped run's configuration and runtime remain frozen.

## Workload and comparison

Hardware: Core Ultra 9 285K, 24 CPU cores, 62 GiB usable host memory, RTX 5090 with
32,607 MiB reported VRAM, PyTorch 2.14.0+cu130. Preserve the width-32, two-layer
CNN actor/critic, CPU collectors, CUDA FP32 learner, 64-action PPO minibatches,
four epochs, shuffle sequence, exact-shape raster buckets, routing budget,
failure reward and best-state curriculum. A batch of 64 factories is distinct
from the 64-action GPU minibatch.

The retained batch-3 update contains 61 admitted factories, 1,952 actions,
nonempty Adam state and both inherited start depths. Update microbenchmarks use
one warmup and three alternating measured repetitions unless specified. Restore
the checkpoint from disk for every cell: Adam CPU step tensors can alias a
loaded state dictionary, so reusing one mutable dictionary invalidates an
otherwise independent comparison. The first trial exposed that benchmark bug;
it was discarded and the corrected trials below all matched.

The end-to-end comparison runs each frozen runtime sequentially on the idle
server with the same input manifest and retention selection `[1, 3, 4]`. The old
path writes and then deletes batch-2 experiences; the new path never writes
them. Original launch readiness ran concurrently with earlier training and took
230.12 seconds; that timing is excluded from the speedup claim.

## Why the batch occupied 1.4 GB

The retained batch serializes to 1,365,434,683 bytes with pickle protocol 5 and
92,893,856 bytes with the existing compressed Torch archive. Its unique NumPy
objects contain 1,355,981,328 bytes. Counts below assign already-shared
actor/critic objects only once; counting every reference would incorrectly
double the total to 2.61 GB.

| Arrays | Decimal MB | Reason |
| --- | ---: | --- |
| Current and best 28-channel rasters | 406.1 | Full spatial inputs at every selection |
| Graph nodes, edges, features and node coordinates | 453.7 | Full current/best graph inputs |
| All candidate actions, coordinates and member features | 491.2 | PPO recomputes the distribution over every legal candidate |
| History and context | 5.0 | Per-selection temporal and scalar inputs |

Across successive selections, **781,986,448 bytes (57.7%) repeat exactly within
their episode**. Only 573,994,880 array bytes have distinct dtype/shape/content.
About 87.0% of raster elements are zero. These are repeated dense model inputs,
not 1.4 GB of new factory information or selected-action labels.

Native-v4 uses protocol-5 out-of-band buffers, deduplicating equal bytes after
SHA-256 lookup plus exact byte-array comparison. Batch 3 occupies 574,332,244
shared bytes including alignment and about 9.3 MB of socket metadata. Collectors
copy unique bytes into an anonymous Linux memfd; the learner maps them directly.
Arrays own mapping lifetime. No named shared-memory files, disk-backed delivery
or per-episode resume cache are needed. This also avoids repeatedly moving the
entire expanded batch through ordinary pipe buffers.

## Input and update measurements

| Retained batch-3 update variant | Median seconds | Exact full continuation |
| --- | ---: | --- |
| Frozen file loading and preparation | 28.655 | Yes |
| In-memory input, original preparation | 17.403 | Yes |
| One-ahead CPU preparation and asynchronous copies | 14.150 | Yes; one measured repetition |
| Immutable feature cache plus overlap and asynchronous copies | 12.882 | Yes |

The final update microbenchmark is 2.22x faster (55.0% less time). These timings
exclude collector delivery. In the supervised eight-worker readiness run,
batch-3 update time was 30.771 seconds before and 15.816 seconds after; complete
batch time was 38.363 versus 23.393 seconds. Supervisor/observer work, allocation
history and input storage differ from an isolated retained-update process.
Do not substitute the 12.882-second number for complete training-batch latency.

The old loader gave a seekable Torch archive reader a gzip stream. The sampled
archive incurred 22 seeks, including 11 backward seeks. Whole-batch decoding was
about 11.1 seconds; inflating once into a seekable byte buffer took about
2.3 seconds including Torch decoding. That improvement remains in explicit
offline audit loading. Normal learning uses neither Torch decoding nor gzip.
This decoder-only probe ran during the former training session; use the idle
update/end-to-end comparisons for speedup claims.

The learner calculates mask balancing and critic observation summaries once per
update instead of repeating immutable work across training/statistics passes.
A single preparation thread builds at most one future CPU minibatch while the
current GPU work runs. Per-tensor nonblocking CUDA copies reduced a representative
426-tensor / 104.55 MB transfer from about 9.26 to 7.42 ms. Coalescing those tensors
into three pinned allocations took 12.88 ms and was rejected; its implementation
is confined to the diagnostic benchmark.

## Collector count and memory

The final worker comparison ran after all other tests finished. It replayed the
same batch-3 tasks without experience archives, with one warmup and three measured
repetitions per count. Every trajectory and observation matched the retained
reference. An earlier trial overlapped briefly with regression tests and is
excluded from the final timings.

| Workers | Median collection seconds | Peak process-tree RSS, GiB |
| ---: | ---: | ---: |
| 8 | 4.738 | 9.90 |
| 12 | 3.861 | 14.30 |
| 16 | 3.354 | 18.67 |

Sixteen workers reduce collection time by 29.2% relative to eight. Full supervised
readiness peaked at 22.94 GiB with sixteen workers, compared with 14.19 GiB for
eight and 15.75 GiB for the frozen baseline. All remain below the unchanged
32 GiB process-tree limit. These RSS sums include mapped/shared pages as reported
per process; they are conservative accounting, not unique physical-memory usage.
More collectors are not automatically safe for later, larger factory batches.

## Durability, verification and remaining cost

The [native-v4 contract](../designs/2026-09-10-ppo-data-supply.md) removes
completed-episode reuse within an uncommitted phase. Atomic durable commits still
cover actor/critic, Adam, RNG and the next curriculum plan. Normal batches retain
compact identity/results and required best-state bundles; only explicitly
selected diagnostic batches retain full experiences and traces. Older runs use
their original frozen runtime.

An initial native-v4 integration run found that curriculum publication still
read a suppressed full episode trace. Publication now validates the authoritative
compact result row and best-geometry digest. That failed trial retained the
batch-1 commit even though batch-2 computation had already happened, confirming
the commit boundary did not move accidentally. The corrected run completed all
four batches, including the archive-free second batch.

The native audit checked 256 retained/evaluation results, the absence of archives
for 64 transient attempts, all 32 inherited starts, consecutive parent lineage,
and exact batch-3 reconstruction from nonempty Adam. Every batch's checkpoint
and epoch fields also matched both the original reference and the new idle
baseline. Collector-count comparisons fingerprint every nested observation,
reward and recorded probability, not just final model weights.

The public CLI interruption check stopped during batch-2 learning. The durable
pointer remained at the hash-verified batch-1 checkpoint. `resume` created new
attempts for all 64 tasks with identical logical task identities, and all four
completed batches/evaluations matched the uninterrupted sixteen-worker run.
Interrupted diagnostic records and checkpoints remain selected local evidence.

Local verification passed all 77 PPO tests (three Linux/CUDA-specific checks
skipped on macOS); the server passed all 77 with the native library and CUDA,
without skips. `cargo check --workspace`, `cargo test --workspace`, Git whitespace
checks and the repository artifact/size policy check passed. No Rust code changed.

The remaining model execution is not GPU-saturating matrix multiplication. A
warmed resident 64-action minibatch took 59.49 ms without profiling. Instrumented
execution issued 13,876 CUDA kernel launches; launch calls consumed 31.78 ms of
CPU time. Deterministic indexing backward was a substantial device cost: the
index-put operator group reported 16.90 ms, including an overlapping 13.55 ms
small-stride kernel subgroup. These nested times must not be summed. Small
operations, exact-shape buckets and dispatch remain follow-up work; no claim of
full CPU/GPU saturation is made.

## Reproduction and retained evidence

Use the explicit native-v4 readiness commands in the
[pilot README](../../tools/factory-edit-pilot/README.md#native-ppo-data-supply-and-recovery).
The following standalone diagnostics take fresh outputs and preserve frozen
source inputs:

```sh
python tools/factory-edit-pilot/profile_ppo_payload.py --run "$REFERENCE_RUN" --batch 3 --output "$FRESH_OUTPUT/payload.json"
python tools/factory-edit-pilot/profile_ppo_data_supply.py --run "$REFERENCE_RUN" --batch 3 --output "$FRESH_OUTPUT/decoding" --repeats 3
python tools/factory-edit-pilot/benchmark_ppo_data_supply.py --phase update --run "$REFERENCE_RUN" --baseline-runtime "$FROZEN_BASELINE_RUNTIME" --batch 3 --output "$FRESH_OUTPUT/update" --repeats 3 --seconds 900
python tools/factory-edit-pilot/benchmark_ppo_collection_supply.py --run "$NATIVE_V4_RUN" --output "$FRESH_OUTPUT/workers" --workers 8 12 16 --repeats 3
python tools/factory-edit-pilot/compare_ppo_supply_runs.py --before "$BASELINE_IDLE_RUN" --after "$NATIVE_V4_RUN" --output "$FRESH_OUTPUT/comparison.json"
python tools/factory-edit-pilot/check_ppo_batch_recovery.py --config tools/factory-edit-pilot/ppo-data-supply-readiness.json --reference "$NATIVE_V4_RUN" --output "$FRESH_OUTPUT/recovery"
```

Run these under the recorded server virtual environment. The baseline runtime is
`/home/ashrimp/Srv/aic-ppo-cnn2-curriculum-64-12h-20260910`. Its original reference
is `artifacts/factory-edit-pilot/cnn2-curriculum-readiness`; its idle baseline is
`artifacts/factory-edit-pilot/data-supply-idle-baseline`. Native-v4 runtime and
readiness evidence live under `/home/ashrimp/Srv/aic-ppo-data-supply-20260911-r2`.
Earlier isolated update measurements and their Python source snapshots remain
under `/home/ashrimp/Srv/aic-ppo-data-supply-20260910`. Raw results, traces,
checkpoints and source snapshots remain ignored artifacts; only this summary,
source diagnostics and compact provenance belong in Git.
The [provenance manifest](2026-09-11-ppo-data-supply-provenance.json) identifies
reports, protocol files, source snapshots and the stopped training checkpoint.
All five frozen comparison/recovery protocols passed a final runtime-source
hash check. After diagnostics exited, GPU use returned to 0% / 34 MiB.
