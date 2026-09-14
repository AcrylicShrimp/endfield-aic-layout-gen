# Masked collection throughput diagnosis

## Scope and continuity

Continue the completed batch-size study by attributing collection cost and
improving measured new environment transitions per minute. The retired long run
stays stopped. Reuse 128 logical streams, 16 physical workers, 32-action chunks,
64-action episodes, minibatch 256, four PPO epochs and cold seed 149. Reuse the
actual input bank from the preceding cached-input comparison, preserving its
generation identities and all evaluation geometry reservations.

| Existing benefit | Preservation and check |
| --- | --- |
| Ordered central batched policy and persistent native workers | Keep stream/depth sampling order, frozen behavior and native action budgets |
| Compact shared arrays and direct compressed persistence | Preserve transport bounds, tensor values, descriptor lifetime and ordinary publication |
| Immutable features and cached replay branches | Reuse existing preparation and storage; compare any changed representation exactly |
| Actual-input identity, evaluation separation and recovery | Use the existing input loader, native final validation and checkpoint restore |
| Bounded diagnostic overhead | Profiling is explicitly enabled only for a four-update probe; retain summaries without trajectories |

`diagnostics/masked_collection_profile.py` instruments only diagnostic processes.
Per-collection nested timings separate sampling, begin/step/finish exchanges,
command encoding/sending, receiving, decoding and selector waits. Representative
worker 0 function profiling includes response encoding/sending as well as command
execution. Worker times are aggregate CPU-side contributions, not additive
coordinator wall time. Function profiling changes timing, so throughput adoption
requires an unprofiled normal-run measurement including ordinary persistence.

The isolated server root is `/home/ashrimp/Srv/aic-collection-opt-20260914`.
The initial diagnostic stopped before update 1 because its relocated source root
did not match the old input store's absolute catalog locators. The original bank
was preserved; a separate hardlinked store keeps actual input bytes and binds
the identical catalogs under the new root. The retry is `artifacts/profile-v2`.

## Initial attribution and the missing ownership boundary

In the cached 128-stream profile, the last two collections took 21.856 seconds.
Nested coordinator timings attributed 10.650 seconds (48.7%) to response decoding,
6.311 seconds (28.9%) to selector waits and 1.722 seconds (7.9%) to policy sampling.
These are diagnostic timings, not the unprofiled throughput denominator.
Across all four updates, NumPy buffer reconstruction appeared 1.825 million times
in the coordinator profile. The representative worker spent 8.568 seconds in
Python preparation, including 1.786 million candidate feature additions. Its
explicit native step timer was 1.546 seconds; that timer excludes native
observation export and must not be described as the entire environment cost.

A temporary Python port-bank representation reduced the eight-stream message
from 732 array buffers to 96 while retaining identical values. The normal pilot
reduced collection by 6.1%, but total new sample throughput was about 14,222/min,
versus 14,323/min in the retained eager baseline. Different early KL stops also
changed optimizer work. This did not establish an overall improvement, and the
temporary production representation was removed. Its patch and microbenchmark
remain disposable evidence under `target/collection-opt-partial/`; the original
server evidence is under `aic-collection-opt-20260914/artifacts/port-*`.

The subsequent boundary audit confirmed an omitted established requirement:
the old complete-factory learner received native numerical observations and
selected retained candidates by handle/index. The blueprint and declarative
learners had reintroduced full observation JSON and Python candidate/features
on every action. The native editor and generator were still present. Restoring
this boundary, rather than adopting the partial port-bank change, is the final
implementation described in
`../designs/2026-09-14-declarative-native-ownership-restoration.md`.

## Native restoration verification

Rust now owns root commands, compatible port domains, masks and feature
construction. The normal worker submits a candidate handle, root index and
conditional port indices. Owned numerical arrays cross the existing shared
transport; small outcome metadata crosses PyO3 directly. Changed physical states
move into bounded native chunk storage and are exported at chunk completion for
successful-segment identity. Detailed observations remain an explicit trace
option. Evaluation uses the same numerical observation/selection API.

The new native-boundary tests cover candidate order, every feature and mask,
selected commands, stale/foreign/consumed handles, rejection before charging,
array lifetime, lazy metadata transport, sparse archive likelihood, preparation
credit across a restored chunk, nonempty Adam replay, partial failure evidence
and evaluation with/without traces. All 79 declarative tests passed on the Linux
release library; locally 77 passed and two Linux/CUDA checks were skipped.
`cargo check --workspace`, all 573 active workspace tests and formatting passed.

An explicit diagnostic admitted 128 archived actual factories and checked 212
observations, including eight successive edits on the first 12 factories.
Every numerical field, attempt feature, ordered root command, mask and port
rotation matched the Python oracle bit for bit. The independently sampled
elapsed-time feature was aligned only for this comparison. Deterministic CUDA
sampling then produced identical choices, log probabilities, values and entropy;
the native selected commands also matched. This does not assert identical
wall-clock-dependent trajectories across separate training runs.

## Normal execution measurements

The isolated native root is `/home/ashrimp/Srv/aic-native-learning-20260914`.
Rebuilding the library changes the generator provenance contract, so the first
normal arm generated its own actual inputs through the unchanged normal loader.
All 243 generated inputs matched the prior bank's actual input, request,
geometry and generation-seed identities. The original bank was not relabeled.

The first two updates are warmup. The denominator for the last two updates uses
the interval between committed update records and includes collection, PPO,
ordinary checkpoint/source publication and the intervening work. Replay draws
are excluded from the numerator of 8,192 new transitions.

| Input condition | Before, new samples/min | Native, new samples/min |
| --- | ---: | ---: |
| Fresh generation included | 12,210 | 14,845 |
| Actual input prefix cached | 14,323 | 18,584 |

The fresh native arm took 33.111 seconds for the measured updates versus 40.254
seconds before (+21.6% new sample throughput). Measured CPU usage was 8.64% of
24 cores versus 12.31%; GPU utilization was 22.59% versus 14.02%. Peak process-tree
RSS was 25.34 GB versus 26.82 GB. Both measured pairs used one then three actor
epochs. All final retained evaluation factories passed native validation.
These are bounded individual runs, not a mature-policy quality or statistical
speedup claim.

The uninterrupted cached native arm measured 26.449 seconds versus 34.316 seconds
before (+29.7% new sample throughput). Collection fell from 20.797 to 11.601
seconds across the measured pair (-44.2%). PPO took 9.894 seconds versus 8.714;
publication took 2.433 seconds versus 3.658. Thus the observed gain comes from
collection/publication, not a faster optimizer. Actor KL stopping selected one
then two epochs, compared with one then three before. CPU usage fell from 11.73%
to 7.41%; GPU utilization was 21.40% versus 20.01%. Peak process-tree RSS was
25.30 GB versus 26.76 GB. Retained output was 126.24 MB versus 123.73 MB; the
ordinary output bounds remain in force.

A separate cached arm requested a stop after update 1. The normal runner honored
its established complete-wave contract and paused after update 2, then resumed
to update 4 with 128 streams and valid final evaluation results. Checkpoint 1
contained all 128 live native sessions at 32 actions; checkpoint 2 contained the
completed 64-action episodes. Mid-episode restore/credit was tested directly in
the native worker tests. The pause/resume arm is excluded from throughput
comparison because record modification times would include its idle interval.

## Subsequent profile and remaining bottleneck

The final four-update diagnostic is `artifacts/after-profile`, with results under
`artifacts/after-profile-telemetry`. Its last two collections took 12.125 seconds,
versus 21.856 in the preceding profile. The representative worker's numerical
observation path took 0.256 seconds over 1,576 calls, including 0.176 seconds in
the native `observe` method, compared with 8.568 seconds over 1,568 Python
preparation calls before. Native indexed stepping took 1.669 seconds over 1,536
calls. Python candidate/feature construction is absent from normal collection.

| Measured collection phase | Before profile | After profile | After share |
| --- | ---: | ---: | ---: |
| Response restoration | 10.650 s | 8.488 s | 70.0% |
| Selector wait | 6.311 s | 1.081 s | 8.9% |
| Policy sampling | 1.722 s | 2.018 s | 16.6% |

NumPy buffer reconstructions fell from 1,824,842 to 240,158 across the complete
coordinator profile (-86.8%). Nevertheless, response restoration remains the
largest collection cost. Its timing includes object creation, array restoration
and any garbage collection charged within that call; it does not establish that
raw array copying alone consumes 70%. Step exchanges took 6.981 seconds and
chunk-finish exchanges 2.833 seconds. Those nested totals overlap restoration
and waiting and must not be added to the table as independent phases.

The next performance investigation should therefore isolate coordinator
allocation/restoration and chunk-finalization costs. The current work restores
native ownership and measures the remaining boundary; it does not claim to have
eliminated interprocess communication. All diagnostic processes were confirmed
stopped afterward. The retired long runs remain stopped.

## Evidence and reproduction

`2026-09-14-native-ownership-evidence.json` records the exact measured native
library, matching implementation hashes, launch/evidence references, compact
normal results, final profile phases and verification counts. All 256 actual
inputs in the cached prefix match the old bank's input, request, geometry and
generation-seed identities. Local copies of the compact raw results are retained
under ignored `artifacts/native-ownership-20260914/`.

The normal probes reuse `operations/masked_long_training.prepare(...,
probe=True, probe_config=config)` with the preceding 128-stream launch's
configuration, the existing reserved development panel, a new output path and
the rebuilt native library. The fresh arm points `training_input.directory` at a
new bank; the cached arm preloads its first 256 canonical slots with the existing
`Runtime.inputs.acquire(slot % 128, slot // 128)` before measurement. The native
input bank has its own authentic generator contract. No provenance bypass or
training traces are enabled.

From the server root, the measurement commands were:

```sh
tools/factory-edit-pilot/.venv/bin/python \
  tools/factory-edit-pilot/diagnostics/masked_cold_profile.py \
  --output artifacts/cached-steady --measurement artifacts/cached-steady-telemetry \
  --environment "$AIC_TRACKING_ENV"
tools/factory-edit-pilot/.venv/bin/python \
  tools/factory-edit-pilot/diagnostics/masked_cold_profile.py \
  --output artifacts/after-profile --measurement artifacts/after-profile-telemetry \
  --environment "$AIC_TRACKING_ENV" --profile
```

`AIC_TRACKING_ENV` identifies the existing local environment file; probe tracking
is disabled. These commands require freshly prepared, distinct probe outputs.
`masked_batch_scale.summarize` computes the ordinary committed-update metric.
Do not apply its file-time interval to an arm paused at the warmup boundary.

The explicit actual-input oracle comparison is available as:

```sh
tools/factory-edit-pilot/.venv/bin/python \
  tools/factory-edit-pilot/diagnostics/declarative_native_parity.py \
  --launch artifacts/fresh-normal/launch.json --input-bank "$AIC_REFERENCE_INPUT_BANK" \
  --output artifacts/native-parity-new.json --factories 128 --deep-factories 12 --steps 8
```

Its archived-input admission is diagnostic-only. The equivalent 128-input
comparison passed before the normal measurement, and the packaged CLI was
subsequently exercised on one actual factory for eight selections.
