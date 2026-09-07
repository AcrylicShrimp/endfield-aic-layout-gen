# Fixed-factory edit pilot

An isolated first learning experiment, authorized by the user after the collaborative design
phase. See the [execution contract](../../docs/designs/2026-09-07.02-fixed-factory-pilot-execution.md)
and [pilot hypotheses](../../docs/designs/2026-09-07.01-factory-edit-learning-pilot.md).
The [first result](../../docs/reports/2026-09-07.00-first-cpu-factory-edit-pilot.md) records the
completed 65,536-command seed, outcome/reward curves, learned edit, and interpretation limits.

The model is a 100,898-parameter CNN plus per-object MLP, with shared policy/value features
and a typed local-command scorer. PPO starts from fresh weights. There is no GNN, demonstration
pretraining, route-search helper, repair macro, or production fallback. All episodes start from
the same complete factory; only learner state persists. The first run has one training seed,
65,536 commands, and 128 commands per episode. It does not test generalization.

The deliberately narrow geometry domain is non-crossing belt paths with one source/demand per
commodity network. Four facilities produce a catalog battery from two catalog-external raw
materials. Rates, rotations, footprints, ports, and capacity come from runtime catalog files.
The executor supports the seven parent commands using the local domain in the execution contract;
the policy scores all mechanically proposed commands rather than an outcome-ranked shortlist.
Automatic final checking retains a complete best result, including the unchanged root if needed.

`case.json` is a compact human-authored input fixture retained because the identical starting
geometry and complete external obligations define this overfit task. It includes one improving
witness for environment checks only. `run.json` is the prospective run configuration. Neither
file is a generated solver/training result. No historical run artifact is a required input.

The current CPU default is four PyTorch intra-operation threads, selected by a local timing
comparison; the first published result used one. Inter-operation threads remain one. This
keeps PyTorch's standard convolutions, autograd, model size, PPO settings, and command budgets.
`updates.jsonl` now separates batch preparation, forward/loss, backward, and optimizer time.
See the [CPU timing report](../../docs/reports/2026-09-07.01-factory-edit-cpu-speed.md).

From the repository root:

```sh
uv sync --project tools/factory-edit-pilot --locked
tools/factory-edit-pilot/.venv/bin/python -m unittest discover -s tools/factory-edit-pilot -p 'test_*.py' -v
tools/factory-edit-pilot/.venv/bin/python tools/factory-edit-pilot/run.py check --case tools/factory-edit-pilot/case.json --config tools/factory-edit-pilot/run.json --output artifacts/factory-edit-pilot/preflight
tools/factory-edit-pilot/.venv/bin/python -u tools/factory-edit-pilot/run.py train --case tools/factory-edit-pilot/case.json --config tools/factory-edit-pilot/run.json --output artifacts/factory-edit-pilot/cpu-100898-seed17
```

An existing experiment directory is not overwritten. Use a new output directory for a new run.
The training command records catalog/source/configuration hashes and actual package versions,
raw episode/evaluation/update rows, complete checkpoint state, selected layouts/traces, and PNG/SVG
plots. Disposable preflight output and retained experiment evidence are ignored under `artifacts/`.
Checkpoints may include non-replayable interrupted rollout state; do not delete them on the
assumption that a seed can restore an interrupted stream.

To select CPU threads on another host, supply a trusted local checkpoint as an explicit input:

```sh
tools/factory-edit-pilot/.venv/bin/python -u tools/factory-edit-pilot/benchmark.py --case tools/factory-edit-pilot/case.json --config tools/factory-edit-pilot/run.json --checkpoint artifacts/factory-edit-pilot/cpu-100898-seed17/checkpoint-65536.pt --threads 1 2 4 8 --repeats 3 --output artifacts/factory-edit-pilot/thread-benchmark
```

This records one fixed sampled rollout, resets model/Adam/minibatch RNG before each update
measurement, excludes a warm-up round, alternates setting order, and reports median times.
It retains the exact input tensors under the ignored output directory. Optional `--channels-last`
and `--convolution unfold` / `view-unfold` are benchmark-only experiments; training uses standard
PyTorch Conv2d and autograd. Neither alternate convolution is selected by `run.json`.

## CPU and CUDA learners

The `device` configuration selects `cpu` or `cuda` for PPO updates. Both paths collect
observations, sample actions, and validate layouts on CPU. CUDA runs refresh a CPU policy
snapshot after each complete update, before the next rollout. GPU availability is required
when requested; no automatic CPU fallback is used. The first CUDA comparison uses float32,
with TF32 and mixed precision disabled. See the [comparison contract](../../docs/designs/2026-09-07.03-factory-edit-cuda-comparison.md).

`updates.jsonl` records synchronized wall-clock stages, including CPU preparation and transfers
in `batch`. Synchronization makes completed GPU work visible and adds diagnostic overhead.
These stage clocks are not pure kernel timings. Metadata also records CPU snapshot-copy time,
GPU identity/runtime, peak GPU allocation, and the CPU actor device; checkpoints retain CUDA
RNG state. CPU and CUDA floating-point trajectories need not produce identical trained policies.

For a fixed-input device comparison, run `benchmark.py --input <retained-input.pt> --device cpu`
and `--device cuda` with the same config and threads, each in a fresh output directory. Both
load the exact same observations, weights, and optimizer state. Input tensors and checkpoints
must be trusted local files. The server checkout and its environment live under
`~/Srv/endfield-aic-layout-gen`; long jobs run through SSH in detached tmux sessions with
file logs and exit-status files. No Jupyter service is required.

The `run-server-cpu.json` and `run-server-cuda.json` configs use 16 CPU threads
on the measured server and otherwise share the original learning/evaluation allowance:

```sh
tools/factory-edit-pilot/.venv/bin/python -u tools/factory-edit-pilot/run.py train --case tools/factory-edit-pilot/case.json --config tools/factory-edit-pilot/run-server-cpu.json --output artifacts/factory-edit-pilot/server-cpu-t16-seed17
tools/factory-edit-pilot/.venv/bin/python -u tools/factory-edit-pilot/run.py train --case tools/factory-edit-pilot/case.json --config tools/factory-edit-pilot/run-server-cuda.json --output artifacts/factory-edit-pilot/server-cuda-t16-seed17
```

An initial 24-thread CPU attempt exposed an inference slowdown on elongated inputs and was
interrupted with its checkpoint/partial rollout retained. `rollout_benchmark.py` replays a saved
input's action sequence while interleaving inference, candidate generation, observation, and
validation. It checks every reconstructed observation against the saved tensors after timing.
This complements the contiguous-inference microbenchmark. On actual interrupted-run inputs,
16-thread inference was about 19x faster than 24-thread inference; 16 also won the larger-input
update comparison. `benchmark.py --actor-threads` isolates inference threads when diagnosing
update settings. Training uses the simpler single selected thread count.

Independent acceptance of an exported factory, without running the environment or policy:

```sh
tools/factory-edit-pilot/.venv/bin/python tools/factory-edit-pilot/acceptance.py --case tools/factory-edit-pilot/case.json --layout artifacts/factory-edit-pilot/cpu-100898-seed17/training-best.json
tools/factory-edit-pilot/.venv/bin/python tools/factory-edit-pilot/plots.py --case tools/factory-edit-pilot/case.json --output artifacts/factory-edit-pilot/cpu-100898-seed17
tools/factory-edit-pilot/.venv/bin/python tools/factory-edit-pilot/audit.py --case tools/factory-edit-pilot/case.json --output artifacts/factory-edit-pilot/cpu-100898-seed17 --source-commit 42ade74
```

`learning-curves.png` separates frozen-policy outcomes, training/evaluation episode rewards,
and the cumulative training champion. Under this reward, one episode's total equals its final
relative area reduction. `factory-comparison.png` shows initial, representative evaluation,
training-best, and scripted witness layouts with independent complete scores. A witness is
explicitly labeled as unlearned. A flat curve remains a reportable first result.

The completed [same-server report](../../docs/reports/2026-09-07.02-server-cpu-cuda-pilot.md) separates update throughput from final learned quality. `compare.py --runs <cpu-run> <cuda-run> --output <comparison-directory>` renders the paired outcome, reward, and timing evidence.

## Continue a completed CPU run

`run.py train --resume <trusted-final-checkpoint.pt>` restores the completed parent's policy,
Adam optimizer, training RNG states, counters, and retained champion. The new configuration
may change only total commands and evaluation checkpoints. Every episode still starts from
the original factory. Interrupted or intermediate checkpoints, semantic changes, and runtime
changes are rejected. See the [continuation contract](../../docs/designs/2026-09-07.04-fixed-factory-continuation.md).

```sh
tools/factory-edit-pilot/.venv/bin/python -u tools/factory-edit-pilot/run.py train --case tools/factory-edit-pilot/case.json --config tools/factory-edit-pilot/run-server-cpu-continue.json --resume artifacts/factory-edit-pilot/server-cpu-t16-seed17/checkpoint-65536.pt --output artifacts/factory-edit-pilot/server-cpu-t16-seed17-continue
```

Episode/command/update counters are cumulative; raw rows and timing fields cover only the new
segment. Metadata records `resumed_commands`, `segment_commands`, `timing_scope`, and checkpoint
lineage including inherited training time. Keep the parent directory alongside its continuation.

Candidate observations now reuse finite catalog/action one-hot templates and fill owned NumPy
arrays directly. Candidate sorting reuses the same standard JSON encoder. No action pruning,
feature changes, state cache, or validation reduction accompanies this optimization.
`environment_benchmark.py --case <case> --config <config> --input <retained-input.pt> [<input.pt> ...] --reference-source <trusted-git-commit> --output <fresh-directory>` compares every candidate in order, observation byte, and transition against the named historical environment and recorded observations. It alternates implementations after warm-up and retains timing/parity evidence.

The [continuation result](../../docs/reports/2026-09-07.03-fixed-factory-continuation.md) remained flat at 4.545% through 131,072 commands. `plots.py --parent <parent-run>` adds verified parent rows to a separate `learning-curves-with-parent` figure without changing raw records.

The [observation optimization report](../../docs/reports/2026-09-07.04-observation-construction-speed.md)
records exact-input environment reductions, the full CUDA rerun, and numerical repeatability.
`benchmark.py` records updated parameter hashes and differences outside measured clocks.
Its optional `--deterministic` requests deterministic PyTorch/cuDNN algorithms and supplies a
cuBLAS workspace configuration when absent. This is a diagnostic setting; the training runner
keeps its prior default. Exact repeatability within CUDA does not imply CPU/CUDA bit equality
or successful learning.

## Reuse data preparation work

CUDA PPO updates now prepare a collected rollout once and reuse its tensors across epochs.
Each sampled minibatch is cropped to its original maximum raster/candidate dimensions before
selection, preserving samples, ordering, values, and neural workload. Prepared tensor payloads
are bounded to 1 GiB; larger estimates use ordinary minibatch preparation and report that mode.
CPU updates keep ordinary minibatch construction. `updates.jsonl` records `preparation.mode`
and `estimated_rollout_bytes`; all preparation/selection time remains charged to `seconds.batch`.

The environment uses separate 4,096-entry LRU caches for exact typed-command JSON sort strings
and pure candidate encodings. Keys include all relevant fields/coordinates; mutable geometry
and validation results are never cached. Eviction changes only computation cost. Global
`environment_counters` record cache hits, misses, and current entries. Observation arrays own
copies of cached encodings. CPU single-observation inference uses read-only tensor views.

For fixed-input comparisons, `benchmark.py --reference-update <trusted-commit>` alternates old
and current update implementations with identical model/Adam/data/minibatch states.
`rollout_benchmark.py --reference-policy <trusted-commit>` similarly isolates actor inference
while replaying the same environment sequence. See the [preparation contract](../../docs/designs/2026-09-07.06-factory-edit-feature-reuse.md).

The [profile-guided preparation report](../../docs/reports/2026-09-07.05-factory-edit-data-preparation-speed.md)
compares exact-input cost reductions with observed full-run time and memory. `costs.py --runs
<before-run> <after-run> --output <fresh-directory>` produces a non-overlapping cost report.
For a conservative within-episode reuse check, add `--reset-caches-per-episode` to the environment
benchmark; it clears both LRUs and category templates at every episode boundary.

## Immutable internal candidates

`Environment.candidates()` now returns `CandidateSet`, with immutable `Command` entries,
current draft identity/revision, and its geometry index. Training serializes only the selected
command for `step`; `to_dicts()` is reserved for explicit diagnostic domain inspection. External
edit/witness/trace JSON remains unchanged. Observation rejects stale candidate snapshots and
reuses the current snapshot's index. Cached feature encodings include state-dependent terminal
or discard positions. The old dict-candidate/prefix-cache production path has been removed.
See the [cutover contract](../../docs/designs/2026-09-07.07-factory-edit-typed-candidates.md).

`actor_benchmark.py` probes CPU/CUDA execution, inference-mode context, and one/two-episode
batching on fixed retained observations. It reports preparation, completed forward/transfer,
and CPU sampling separately. It does not change production collection or enable deterministic
training. Two-episode GPU batching remains a prospective collection change, not a selected
execution path in this slice.

Immutable candidate measurements and remaining actor costs are recorded in
[the typed candidate report](../../docs/reports/2026-09-08.00-factory-edit-typed-candidate-speed.md).
