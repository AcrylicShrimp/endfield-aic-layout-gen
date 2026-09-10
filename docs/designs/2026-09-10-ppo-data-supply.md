# PPO data-supply optimization contract

Status: implemented and verified. See the
[measurements and recovery checks](../reports/2026-09-11-ppo-data-supply.md).

The user requested higher sustained hardware utilization for long training.
The user subsequently stopped the 64-factory experiment to focus on optimization.
Its frozen runtime and checkpoint remain preserved. Measure and verify changes
in an isolated runtime against its retained readiness inputs.
Improve admitted-action throughput and update latency; utilization alone is not
an acceptance criterion.

## Inputs and outputs

- Input: immutable, hash-identified episode experiences and the preceding
  actor/critic, nonempty Adam and shuffle-RNG checkpoint. Use the retained
  64-factory curriculum readiness batch 3, including both inherited depths.
- Output: the same decoded observations, rewards, credit, shuffled minibatches,
  optimizer boundaries, epoch diagnostics, KL stopping decision and checkpoint
  state. Native construction, action sampling and current-policy ownership stay
  unchanged.
- The user explicitly selected completed-batch recovery: preserve atomic durable
  model/Adam/RNG/curriculum commits; resample the entire uncommitted phase after
  interruption. In-flight experiences are transient and are not a resume input.
  This supersedes the old per-episode reuse requirement in harness-native-v3.
  Existing frozen runs and their retained evidence remain untouched.
- Deliver experiences through bounded process-local IPC, with task/behavior/
  reward envelope checks. Normal batches do not serialize experiences through
  Torch or gzip files. Only explicitly retained diagnostic batches write full
  experiences/traces. Keep compact result records and best geometry required by
  the next curriculum plan. Storage encoding is separate from learner delivery.
- Linux collectors use a private Unix socket and protocol-5 out-of-band NumPy
  buffers. Deduplicate equal buffer bytes within one episode (SHA-256 lookup
  followed by exact byte-array comparison), copy unique buffers into one
  anonymous `memfd`, and pass its descriptor with `SCM_RIGHTS`. The learner maps
  it privately; NumPy bases retain that mapping until batch release. No named
  shared-memory objects or resource-tracker cleanup are required. Platforms
  without `memfd_create` reject array delivery explicitly. Metadata remains
  framed and bounded; buffer offsets, logical sizes and descriptor sizes are
  validated before decoding. Report metadata, logical and unique shared bytes.
  Observations are immutable inputs, including arrays whose equal contents now
  share storage. Learner preparation must not mutate recorded observations.
- CPU/GPU tensor movement must preserve values, dtypes, shapes and tree structure.
  Exact-shape raster buckets and per-factory graph membership remain unchanged.
  Do not change minibatch size, shuffle order, FP32 policy or PPO hyperparameters.

## Measurement stages

Separate file read/hash, decompression, deserialization, credit calculation,
minibatch preparation/transfer, forward/backward/optimizer and epoch statistics.
Torch archive readers seek; test whether a gzip stream repeats decompression
when supplied directly to `torch.load`. Compare to one sequential inflation into
a seekable memory buffer before considering a storage-format change.

Measure GPU input movement independently of model math. Candidate follow-ups
are coalesced transfers, reuse of immutable preparation and bounded preparation
overlap. Adopt only measured improvements with equivalent outputs and acceptable
peak memory. Do not overlap next-policy rollouts with the preceding update or
introduce stale behavior trajectories.

The diagnostic CLI takes an explicit frozen run, training batch, fresh output
directory and repetition count. Its report identifies source/protocol/checkpoint/
experience hashes, hardware, workload, per-stage timings, memory, numerical
equivalence and concurrent-training limitations. Retain raw outputs under ignored
`artifacts/`; commit concise findings and compact provenance. Diagnostic baseline
implementations remain confined to the comparison script.

## Failure and verification contracts

- Invalid in-memory envelopes fail before optimizer mutation. A missing, damaged
  or hash-mismatched explicitly retained audit experience fails with its file path.
  Do not treat an audit failure as permission to substitute another trajectory.
- The harness schema changes to native-v4. Old runs must use their frozen runtime;
  no internal adapter automatically restores v3 episode-reuse behavior. A v4
  restart restores the last committed checkpoint and resamples its next phase.
- Preparation must reject invalid execution bounds. Background preparation, if
  adopted, has bounded outstanding work and propagates exceptions to the learner.
- Resource limits and finalization remain owned by the existing supervisor.
  Measure any additional memory; do not assume spare VRAM implies a safe full
  dataset cache.
- Compare decoded nested arrays/tensors exactly against the original decoder.
  Reconstruct the retained CUDA update from nonempty Adam and require exact
  actor/critic/optimizer/shuffle hashes and epoch statistics.
- Run PPO regressions, workspace checks/tests and the actual diagnostic CLI.
  Verify a fresh native readiness run if normal harness collection or execution
  integration changes. Record any remaining bottleneck rather than claiming
  complete CPU/GPU saturation.
