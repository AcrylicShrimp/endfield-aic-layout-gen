# Blueprint execution primitives

The blueprint research runner now has a persistent native worker pool, parallel
per-factory evaluation and a packed CPU/CUDA PPO/SIL update path. This is the
implementation prerequisite for the
[replay-balance experiment](../designs/2026-09-13-blueprint-replay-balance-experiment.md).
It does not establish the required end-to-end speedup or authorize launching an
unmeasured main run. Fork/admission orchestration and complete-cycle readiness
remain the next slice.

## Worker and learner contract

Each worker owns exactly one `Stream`, including its native session, counters,
input identity and unfinished draft. The coordinator owns the ordered CPU policy
sampler. A frozen CPU behavior copy evaluates the eight disjoint graphs together;
selected actions then execute concurrently in worker-owned native sessions.
Collection returns the same step-major, stream-major sample order as the serial
reference. Optimizer boundaries preserve live drafts and their true episode horizon.

`blueprint-worker-message-v1` commands carry a request ID and an operation:
`initialize`, `begin`, `step`, `finish`, `snapshot`, `model`, or `evaluate`.
Initialization binds configuration, native library, stream index and optional
saved stream. A step carries the selected candidate index, old log probability,
old value and entropy. Finish supplies the bootstrap value. Responses identify
the request and worker index and carry either prepared graph input, completed
chunk evidence, a saved stream, or an evaluation row, plus worker time.
`close` ends the private process without another response.

Prepared arrays use the existing bounded Linux memfd transport with deduplicated
out-of-band NumPy buffers. Mappings own their arrays until their final references
are released. A worker transmits the next graph each step and its complete chunk
once, rather than retransmitting its growing trajectory every step. Catchable
worker failures retain a partial native/sample artifact. A disconnected or lost
worker raises an error; the pool cannot publish or advance a learner checkpoint.
An uncatchable kill has only the coordinator's previous durable boundary.

The caller selects distinct available CPU IDs, one per stream. Each worker pins
itself to its assigned CPU and uses one Torch thread. Initialization verifies live
PID uniqueness and effective affinity. Main-launch quota, physical-core allocation,
resource limits and the measured throughput gate belong to the forthcoming
readiness/launch contract; the pool alone does not establish those claims.

Independent fresh-input slots now generate under separate slot locks. Manifest
and catalog initialization hold only a short shared lock. Paired consumers of the
same slot still wait for one atomic publication and receive the same actual input.
An interrupted slot is preserved and rejected, rather than silently regenerated.

CPU preparation precomputes graph segment lengths. Device-dependent allocations
follow their input tensor's device; model parameter names and ordering are
unchanged. The update prepares one next minibatch on a CPU thread while the
learner executes the current batch. It keeps separate PPO and imitation forwards,
true-terminal GAE, shared encoder value gradients, positive detached-value SIL
weights, and equal weighting of present stages inside each replay sequence.
Balanced replay draws four sequences per stratum with replacement, then uses a
separate deterministic shuffle RNG. Uniform replay keeps the historical RNG
formula. Neither replay RNG advances PPO shuffling or policy sampling.

Imitation diagnostics use values already computed for the loss. Per-origin,
stratum and stage counts/weight sums transfer after the update. KL stopping freezes
actor and imitation updates together while the critic-head tail preserves actor
parameters and actor Adam moments. Full behavior recomputation is an explicit
test/readiness check, not an automatic extra pass in the optimized update.

The serial reference shares the extracted native step/finalization functions with
the pool. This keeps physical validation, witness collection and terminal counting
in one implementation. Historical completed experiments retain their frozen
runtimes; no old checkpoint is silently loaded under a new experiment protocol.

## Verification

- Local existing blueprint suite: 52 tests, 48 passed and four environment-gated
  skips at that run. The explicit local fresh-input suite passed all three tests.
- Linux execution suite: all three tests passed, including eight real workers,
  shared-buffer lifetime, recorded probability/value recomputation, live-draft
  restoration, true fixture termination, worker loss, parallel evaluation with
  unchanged training RNG, and parallel fresh-input publication/restoration.
- CPU/CUDA update suite: all five tests passed on the selected server environment.
  They cover balanced composition, historical uniform draw/Adam agreement,
  unequal stage sizes and zero weights, no direct SIL critic gradient, and exact
  update replay from nonempty Adam on each device with a forced KL critic tail.
- Packed CUDA probabilities, values and shared encoder gradients agree with CPU
  within the registered test tolerances. CPU old/new optimizer tensors agree
  within `rtol=1e-4, atol=2e-6`; no bit-exact old/new optimizer claim is made.
- `cargo check --workspace` and `cargo test --workspace` passed: 564 tests passed,
  five intentionally ignored. Rust and production planner semantics were unchanged.

The small execution tests use shortened mechanical horizons. Their success does
not replace the required real eight-update fork, 64-action episode checks, parent
admission, original 32-case evaluation budget or complete four-update timing cycle.
The mechanical positive-gate fixture changes a critic bias only in the test;
its weights and labels never enter main training.

Reproduce the server tests from the isolated replay checkout with the selected
Python environment, `AIC_NATIVE_LIBRARY` pointing to its release native library,
`AIC_RANDOM_INPUT_TESTS=1`, and `CUBLAS_WORKSPACE_CONFIG=:4096:8`:

```sh
python -m unittest discover -s tools/factory-edit-pilot -p test_blueprint_execution.py
python -m unittest discover -s tools/factory-edit-pilot -p test_blueprint_execution_update.py
```

No full-cycle timing, matrix forecast, transfer estimate or new learned factory
improvement is reported by this implementation slice.
