# Restore native ownership in declarative learning

Status: implemented and verified. This restores the ownership requirement of
`2026-09-10.08-full-factory-native-environment.md` for the current declarative
action contract. The user identified the omission after collection profiling.
The old complete-factory editor is not a fallback for the new action semantics.

The old `FullFactoryEpisode.observe()` exported numerical inputs and a candidate
handle; `step(handle, index)` retained candidate commands in Rust. Blueprint and
then declarative learning reintroduced Python feature/candidate construction and
full observation JSON on every step. The engine and fresh generator remain native;
the ownership regression is in the learning observation/action boundary.

## Required boundary

Rust owns the declarative session, ordered root candidates, compatible port
domains, structural band masks, graph/port feature construction and current
physical state. Native preparation exports owned numerical arrays, compact
immutable port descriptors and a selection handle. Python selects a root and
conditional port indices with the existing ordered CPU RNG and batched GPU heads.
The worker submits those indices to the retained native domain. It does not
reconstruct a native command or a complete physical observation in the hot loop.

The central process receives only numerical learning inputs and the metadata
needed for conditional decoding. It does not reconstruct every candidate command
or every port tensor view. Existing persistent workers, shared array transport,
resource guards, behavior freezing and minibatch preparation remain in use.
Small control messages still cross process boundaries; native ownership does not
require eliminating all communication or replacing the worker pool.

Native steps return selected-action metadata, reward/status, counters and useful
diagnostics directly through PyO3. JSON remains a cold boundary for actual input
admission, native checkpoints, explicit diagnostic exports and final witnesses.
Physical states needed for successful-segment identity and optional geometry
scoring are exported at the chunk boundary. Detailed traces remain opt-in.

## Continuity checks

| Existing requirement | Reuse and verification |
| --- | --- |
| Declarative geometry, directed incidence and charged native budgets | Reuse the existing edit/route/acceptance implementation; compare selected steps |
| Exact candidate order, masks and graph/port feature meanings | Retain the Python builder only as an explicit oracle; compare actual observations and all numerical fields |
| Joint root/port likelihood, without-replacement choices and RNG order | Keep existing heads, ordered sampling and PPO kernel; compare choices, likelihoods and optimizer updates |
| Owned historical inputs | Export owned arrays; later native edits must not mutate prior PPO/replay inputs |
| Complete-wave recovery and actual inputs | Keep native checkpoint schema, source validation, input identity and established restore boundary |
| Successful-segment identity, credit and sparse replay | Preserve required physical states and selected actions; compare buffer admission and compressed restore |
| Held-out evaluation separation and valid retained best | Cut over evaluation too; preserve its seeds, budgets, geometry reservations and final validation |
| Measured execution and memory benefits | Run the actual bounded normal CLI, including publication; compare new samples/min and resource usage |

The preceding port-bank micro-optimization is not adopted as the solution. Its
first normal run reduced collection time by about 6%, but did not establish an
end-to-end sample-rate improvement. Keep that evidence and the collection profile;
restore the omitted boundary before drawing a throughput conclusion. All old
long runs remain stopped; no new long training run is authorized by this work.

## Completed cutover

Normal training workers and held-out evaluation use native `observe()` and
`step_index(handle, root, ports)`. The Python feature builder is an explicit
diagnostic oracle only. Native handles reject stale/foreign/consumed selections
and invalid masked/port indices before charging the editor. Valid selections
reuse the original charged edit, routing, validation and best-retention logic.

Owned arrays preserve historical PPO inputs. Compact root metadata and lazy
conditional port views use the existing shared-memory transport. Sparse replay
keeps the selected command and its branch; it drops live native handles. Changed
physical states move into a bounded native chunk and supply the same replay
identity and preparation credit when exported at completion. Partial failures
retain the available native evidence. Full state JSON remains a cold admission,
checkpoint, explicit diagnostic or final-result boundary.

The native checkpoint schema and complete-wave CLI resume contract are unchanged.
The library must be rebuilt: an older extension fails with a specific
`declarative.native-learning` diagnostic. Existing source/input provenance guards
remain active; rebuilding the extension does not silently relabel an old input
bank's generator contract.

Verification covered exact numerical/action parity on 128 actual factories,
joint likelihood, sparse archives, intermediate worker restoration, nonempty
Adam replay, normal CLI pause/resume, and valid final evaluation. The bounded
normal comparison improved new transitions/minute by 21.6% with generation and
29.7% with a cached input prefix. Subsequent profiling identifies coordinator
response restoration as the remaining collection bottleneck. See
`../reports/2026-09-14-masked-collection-throughput.md` and its compact evidence
manifest for conditions and limits. No diagnostic hooks run in normal training.
