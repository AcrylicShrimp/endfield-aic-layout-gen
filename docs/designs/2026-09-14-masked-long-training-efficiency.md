# Masked long training: execution and storage efficiency

Status: implemented and verified; the 12-hour cold run is launched. The user authorizes a new
12-hour cold masked run after server profiling and optimization. They explicitly
forbid bulk intermediate data during training and unnecessary runtime validation
or recomputation. The completed comparison and paused older main are immutable.

## Scope and retained contracts

Use the tested structural-band mask and existing 40-wide, two-round PPO learner,
32 logical streams, 64-action episodes, 16 persistent physical workers, packed
CUDA behavior/optimization, eight-sequence balanced-available successful replay,
utility retention and 80/10/10 curriculum. Changes target execution, storage and
normal operational overhead, not the learning objective or editing problem.

The actual long run starts from random model initialization with empty optimizers,
replay and training history. Retained trained states may be used read-only for
bounded diagnostic workloads; none initializes the long run. Preserve evaluation
geometry reservations and external game-data loading. Keep model/optimizer/RNG,
episode identity, valid-best outputs, selected inherited starts and usable recovery.

| Existing benefit | Continuity requirement |
| --- | --- |
| GPU batching and one-batch-ahead CPU preparation | Reuse the existing kernel and measured batching; no per-sample GPU calls |
| Persistent workers and owned compact transport buffers | Preserve worker lifetime, stream ordering and closed mmap descriptors |
| Masked joint likelihood and replay inputs | Preserve every numerical feature, candidate index, kind normalization and recorded mask |
| Indexed eviction and hydrated replay cache | Preserve selected sequences, utility updates, admissions/evictions and cache reuse |
| Fresh complete randomized factories and 80/10/10 starts | Keep actual inputs/lineage and native budgets, reduce redundant materialization |
| Committed-output validity and invalid-input rejection | Validate at useful boundaries and reuse stable results; no repeated historical audit |
| Recovery and selected best models | Keep required learning/live-episode state and protected sources without bulk trajectory archives |
| Bounded normal execution | No profiler, tensor trace, full replay audit, directory scan or duplicate full-state save in the hot loop |

## Measurement and implementation boundaries

Reuse the completed cold-mask comparison as the baseline. Its masked runs averaged
2.918 seconds/update in publication. In the seed-71 masked arm, 83 compact replay
sources occupied 1,137,334,297 bytes; five checkpoints used 4,281,832 bytes. The
fresh-input store contained 38,398 files and 695,306,441 bytes, with duplicated
construction/randomization request and layout files. The final state JSON was
3,702,755 bytes after only 128 updates, motivating an explicit long-run growth
check. These observations identify candidates, not established speedups.

Run bounded isolated profiling on the server while all normal learners are idle.
Start with existing replay/checkpoint storage and a normal mature masked cycle.
Profile coordinator and representative workers with compact function summaries;
do not retain large tensor timelines or full training trajectories. Cap each
diagnostic, record attributable timing and bytes, and remove disposable copies
after the required summary and correctness evidence are retained.

Measure any change on identical retained data where possible, then on the actual
integrated normal CLI including persistence, fresh/inherited inputs and recovery.
Distinguish cold hydration, input generation, profiling overhead and steady work.
Use these results to select further optimizations. Stop when remaining significant
costs are necessary work or further changes cannot preserve the stated contracts.

Before starting the 12-hour run, record final source/config, input reservations,
empty initialization proof, resource/output forecasts, compact storage policy,
normal execution timings and a real restore check. End all diagnostic processes
and remove profiler settings. Keep only necessary checkpoints, bounded successful
experience, actual required inputs, compact metrics and selected valid outputs.
The user has authorized the long run; the verification is a technical launch gate,
not a request for another permission round.

## Storage contracts selected from profiling

The server profile found about 30.8 million Torch persistent-storage dispatches
for one 1,003-row replay source, although its immutable tensors already use the
direct ndarray reducer. New compressed archives have an `AICPKL2\0` header and
one protocol-5 object stream, without the redundant Torch ZIP storage wrapper.
The loader retains old Torch-archive support at this file-format boundary. It
inflates once and decodes once, without seeking through gzip or recreating the
object tree before saving. Gzip level 1 remains subject to measured codec cost.

Replay chunk v2 retains the numerical root domain, order, kind normalization,
structural mask and selected conditional port domain. It removes unused action
parameters, unused port branches and completed trajectory exports. Physical
identity and validated witness data needed by unfinished causal credit remain.
Tests compare prepared likelihood tensors and terminal credit across a real
save/restore boundary, rather than requiring irrelevant object fields to match.

Normal state v2 retains the current input wave, a slot frontier and a total count.
Each existing update record owns its actual-input metadata once. A migrated v1
prefix is preserved once; explicit reports reconstruct history, while normal
restore reads only required current inputs. Historical research drivers can
continue to expose their bounded in-memory maps. No additional per-update journal
or historical scan is introduced.

Fresh-input store v2 retains the actual frozen request/layout payload in one
compressed artifact and bounded generation/attempt metadata. Construction and
randomization pass request/layout values in memory through the existing native
constructor and validation engine. The small source-plan file needed by the CLI
remains; redundant wiring stdout, construction layouts and setup traces are not
materialized. The native JSON entry point invokes the same `Episode::new`
validation as the file entry point. Panel export remains an explicit operation.
The new store/source identity is a breaking generation-contract version: existing
banks are read with their recorded format and cannot be silently rebound.

Freshly generated native-validated values are returned directly after atomic
publication. External and restored inputs are authenticated on entry. Catalog,
source and store identities are checked once per persistent loader, not per slot;
attempt ordering, reserved-geometry rejection and slot locking remain required.
Failures preserve a concise diagnostic and any required actual unfinished input,
without turning ordinary generation into a full intermediate-data archive.

## Final execution decision

Immutable candidate kind normalization is prepared once in physical workers and
reused in behavior, PPO and replay batches. Recorded conditional port features
are cached on their input/decision owner and released with that owner. Packing
older inputs preserves their immutability. No ordinary batch recreates a full
Python object tree or regenerates candidate features for saved decisions.

The final server measurement is 5.588 seconds/update (183.246 actions/second),
2.082 CPU cores out of 24 physical cores, and 30.411% average GPU utilization over
the same five-update normal-training window. These utilization values are not
saturation claims. Synchronous environment/behavior dependencies and the retained
PPO workload leave capacity idle; extra work solely to fill it would not improve
iteration speed. A bounded compiled forward/backward experiment improved only
2.3%, so it is not added to normal execution. Preserve the established logical
batch, minibatch, optimizer schedule and environment budgets.

Use initialization seed 149, generation seed 722900000, 43,200 learning-run seconds
with a 600-second complete-wave grace, and the reserved 32-case development panel.
Reserve all 128 previously registered evaluation geometries. Reuse actual fresh
factory inputs only; import no trained model, optimizer, replay or inherited
layout into the cold origin. Keep online W&B metrics, checkpoint every update,
the latest two recovery checkpoints, and selected best all/small/large states.
Evaluate at initialization, every 128 updates and completion. This cadence reduces
monitoring cost during the longer run; it does not change the held-out panel.

The output guard is 16 GiB for the arm, the disk reserve is 16 GiB, and the
process-tree RSS guard is 48 GiB. Actual fresh-input files are outside the arm
ledger, but included in the disk reserve and storage forecast. They are required
resume inputs, not disposable traces. A planning envelope of 8 GiB covers the
observed mature replay, actual inputs, compact metrics, curriculum and protected
outputs with headroom; it is not a worst-case theorem. Resource limits remain
fail-closed if learning produces a larger workload. Normal execution has no
profiler, external utilization sampler or automatic exhaustive completion audit.

See the [server report](../reports/2026-09-14-masked-long-training-efficiency.md)
for measurements, verification and the immutable launch receipt.
