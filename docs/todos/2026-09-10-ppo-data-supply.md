# PPO data supply for sustained training

Status: complete.

Context: live sampling showed substantial CPU/GPU idle capacity. Training batch
4–11 means were 6.52 seconds collection and 32.00 seconds update; the latter
included 12.34 seconds experience loading/credit and 4.46 seconds preparation.
These are early-run measurements, with variable KL stopping, not a benchmark.

Completion criteria:

- [x] Define workload, equivalence, durability and measurement contracts.
- [x] Profile the retained 64-factory input path and identify avoidable work.
- [x] Implement and measure data-supply improvements without PPO changes.
- [x] Verify exact CUDA continuation, regressions and resource behavior.
- [x] Record remaining bottlenecks and commit verified changes.

Activity log:

- 2026-09-10: User requested optimization for very long training. Keep the active
  experiment frozen. Initial median/largest episode probes show about 4.7–5.0x
  faster decoding when gzip is inflated once before Torch deserialization;
  complete-batch measurements and exact comparisons remain pending.
- 2026-09-10: User approved recovery from the last completed batch and transient
  in-memory experiences. The native-v4 contract removes per-episode resume reuse.
- 2026-09-10: User stopped the active training run to focus on optimization.
  Normal `user-stop` finalized at 32 batches / 64,608 actions; checkpoint hash and
  W&B finalization passed. Subsequent GPU benchmarks run without training contention.
- 2026-09-11: Removed normal experience archives, added native-v4 shared-memory
  delivery with exact buffer deduplication, one-ahead preparation and immutable
  feature caching. The final isolated batch-3 update fell from 28.655 to 12.882
  seconds with exact actor/critic/Adam/shuffle/epoch results.
- 2026-09-11: Four eight-worker native batches and two evaluations matched the
  frozen baseline exactly and took 115.20 versus 192.16 seconds on the idle
  server. Native audit covered archive-free batch 2 and all curriculum lineages.
  Subsequent sixteen-worker execution and actual interrupted-batch recovery passed.
- 2026-09-11: Final isolated collection medians were 4.738 / 3.861 / 3.354
  seconds for 8 / 12 / 16 workers, with exact trajectories. Sixteen-worker full
  readiness took 106.09 seconds and peaked at 22.94 GiB, below the 32 GiB bound.
  Updated the future-run configuration; did not restart long training.
- 2026-09-11: Stopped during batch-2 learning, verified the durable batch-1
  checkpoint, resampled all 64 tasks on resume and reproduced all final states
  exactly. All 77 PPO tests passed on the native/CUDA server; local platform
  checks and workspace checks/tests passed. Recorded the
  [report and provenance](../reports/2026-09-11-ppo-data-supply.md).
