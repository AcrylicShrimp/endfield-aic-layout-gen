# Coordinator response restoration throughput

Status: implemented and verified. Continue from native ownership restoration commit
`12394b7`. The remaining profile attributed 8.488 of 12.125 collection seconds to
response restoration. That includes allocation and any garbage collection charged
there; raw array copying has not been established as the cause.

## Scope and continuity

Use the existing 128 logical streams, 16 persistent physical workers, 32-action
chunks, 64-action episodes, ordered CUDA policy/CPU categorical RNG and ordinary
PPO/publication. Reuse the native library and actual 256-input prefix. Keep the
retired long runs stopped. Profile only bounded diagnostic processes.

| Existing benefit | Required continuity check |
| --- | --- |
| Rust-owned commands, masks and feature arrays | No Python feature construction or action reconstruction returns to normal collection |
| Immutable historical PPO and sparse replay inputs | Exact array values, likelihood, sampling and owned buffer lifetime survive later messages |
| Persistent shared-array transport and bounded descriptors | Preserve byte bounds, input rejection, no disk fallback and release file descriptors |
| Native budgets, terminal credit, witnesses and complete-wave recovery | Reuse native/editor/learner semantics; run relevant tests and normal CLI |
| Sparse archives, batched preparation and source validation | Avoid replacing optimized paths; preserve retained sources and authentic input bank |
| Useful normal work and resource ceilings | Measure new samples/min including publication and memory; stop diagnostic processes |

The isolated server root is `/home/ashrimp/Srv/aic-response-opt-20260915`.
`relocate_inputs` links the preceding authentic actual inputs and explicitly
rebases only the catalog locator. Native bytes are unchanged. The first diagnostic
adds generation-specific garbage-collection timing to the existing collection
profile, without changing GC policy or inserting manual collections.

## Evidence-backed change

The new baseline profile's last two collections took 11.895 seconds. Response
restoration took 6.668 seconds, of which 5.015 seconds was garbage collection.
Four generation-2 collections inside response restoration consumed 4.206 seconds
and collected only nine objects. Another generation-2 collection during policy
sampling took 1.561 seconds and collected none. Thus much of the cost previously
charged to NumPy restoration was repeated scanning of live Python objects.

Each numerical input previously reconstructed the same nested port descriptors:
owner/group lists, node/port records, rotation-range dictionaries and lists.
These descriptors are constant for a native session. `Runtime.observe` now
encodes that native descriptor object once, and `PortGroups` transmits the
self-contained encoded descriptor alongside its existing numerical bank.
Receivers reuse descriptors by exact encoded bytes through weak references.
They rebuild the nested objects only when no live matching domain exists.

This is a cache of immutable descriptors, not a new channel/session protocol.
Any message remains independently readable without a preceding handshake. Equal
bytes include all node identities, port choices and rotation ranges; different
domains cannot collide through a hash-only identity. Domains disappear after
their last owner is released. Numerical banks and lazy tensor caches remain
local to each input. Sparse replay continues storing only selected branches.
The GC policy, shared-memory byte checks, fd lifetimes and native library are
unchanged. Legacy full-input archives still pass their descriptor lists to the
existing `PortGroups` constructor.

The bounded microbenchmark uses eight actual streams and retains 512 successive
eight-stream messages per leg, in previous/shared/shared/previous order. It keeps
ordinary GC active and checks identical sampling and descriptor cleanup.
Response restoration fell from 0.984/0.811 seconds to 0.158/0.158 seconds. Shared
array bytes stayed exactly 550,174,720 per leg. Metadata grew only 0.19% to
56,133,632 bytes. Encoding increased from 0.612/0.578 seconds to 0.648/0.649;
therefore adoption depends on the integrated normal measurement, not decode
microbenchmarks alone.

All 81 declarative tests passed on the Linux release library. Locally, 79 passed
and two Linux/CUDA checks were skipped. The new tests check independent-message
sharing, unequal-domain separation, release after the last owner, reconstruction
after cache expiry, isolated array storage and local lazy views. Existing tests
also cover sparse archive likelihood, preparation credit, live worker restore,
nonempty Adam replay, native selection and optional evaluation traces.

## Normal execution

The uninterrupted cached-input arm `artifacts/shared-normal` completed all four
updates. As in the preceding comparison, two updates warm up and the next two
contain 8,192 new transitions. The committed-record interval includes PPO,
ordinary publication and intervening work; replay draws are not new samples.

| Metric | Native ownership baseline | Shared descriptors |
| --- | ---: | ---: |
| New transitions/minute | 18,584 | 24,330 |
| Measured elapsed time | 26.449 s | 20.202 s |
| Collection, measured pair | 11.601 s | 8.364 s |
| PPO, measured pair | 9.894 s | 8.869 s |
| Publication, measured pair | 2.433 s | 2.406 s |
| Mean CPU use, 24 cores | 7.41% | 8.24% |
| Mean GPU utilization | 21.40% | 26.65% |
| Peak process-tree RSS | 25.30 GB | 25.08 GB |
| Retained output | 126.24 MB | 126.19 MB |

New sample throughput increased 30.9%; collection decreased 27.9%. Both measured
pairs used one then two actor epochs and reported the same mean root-candidate
counts. This is a bounded individual-run comparison, not a statistical or mature
policy-quality claim. All retained final evaluation factories passed native
validation. The same native binary, authentic input prefix, resource guards and
ordinary storage path were used. GC was neither disabled nor deferred.

## Follow-up profile

The final bounded profile completed the same four-update workflow. The measured
pair's collection time fell from 11.895 to 9.155 seconds. Total GC time within
collection fell from 6.617 to 4.031 seconds. Inside response restoration,
generation-0 collections fell from 2,265 to 704 and generation-1 collections from
206 to 60; generation-2 collections fell from four to three. An additional long
GC pause previously charged to sampling disappeared.

| Nested phase | Baseline profile | Shared-descriptor profile |
| --- | ---: | ---: |
| Response restoration | 6.668 s | 5.473 s |
| GC inside response restoration | 5.015 s | 4.005 s |
| Policy sampling, including GC | 3.575 s | 1.973 s |
| Step exchanges | 5.297 s | 3.101 s |
| Chunk-finish exchanges | 2.705 s | 3.786 s |

GC and exchange timings overlap other rows; they must not be added as independent
phases. The same-value microbenchmark isolates descriptor reconstruction, while
the normal run and full profile retain all other required payloads and work.
Sharing descriptors removes the repeated nested metadata graph; it does not
eliminate all response allocation or all GC. Chunk-finish exchange now takes
longer than the combined step exchanges in the measured pair. Its cold physical
state/witness handoff and remaining heap scans are the next investigation target.
The profile does not isolate how much of each GC pause belongs to that handoff.

## Verification and reproduction

`cargo check --workspace` and all 573 active workspace tests passed (five ignored).
All 81 release declarative tests passed; the nine ownership tests also passed
after adding the explicit single-domain-per-native-session and legacy constructor
checks. `check-tracked-files.mjs` validates documentation/artifact limits.

`2026-09-15-response-restoration-evidence.json` records implementation/native
hashes, source launches, input-location receipt, normal metrics, microbenchmark
results and profile summaries. Compact raw evidence is retained under ignored
`artifacts/response-restoration-20260915/`. Prior runs remain immutable.

Prepare fresh probe outputs through the existing
`masked_long_training.prepare(..., probe=True, probe_config=config)`, reusing the
previous cached 128-stream configuration and reserved panel. Set the relocated
input directory and unchanged native library in the new root. Run from that root:

```sh
tools/factory-edit-pilot/.venv/bin/python \
  tools/factory-edit-pilot/diagnostics/declarative_metadata_transport.py \
  --launch artifacts/baseline-profile/launch.json --output artifacts/metadata-transport.json
tools/factory-edit-pilot/.venv/bin/python \
  tools/factory-edit-pilot/diagnostics/masked_cold_profile.py \
  --output artifacts/shared-normal --measurement artifacts/shared-normal-telemetry \
  --environment "$AIC_TRACKING_ENV"
tools/factory-edit-pilot/.venv/bin/python \
  tools/factory-edit-pilot/diagnostics/masked_cold_profile.py \
  --output artifacts/shared-profile --measurement artifacts/shared-profile-telemetry \
  --environment "$AIC_TRACKING_ENV" --profile
```

The environment file is the existing local runner configuration; probe tracking
is disabled. The baseline profile uses commit `12394b7` production sources plus
the diagnostic GC timing addition. The improved arms use the implementation
hashes in the evidence receipt. Normal execution imports no GC callback or
microbenchmark. All diagnostic processes were stopped after completion; retired
long training remains stopped.
