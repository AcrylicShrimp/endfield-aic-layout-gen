# Complete bounded success-arrival capture

> Withdrawn from scientific selection on 2026-09-14 at the user’s request.
> Historical execution evidence only; do not use these results or recommendations
> to select a training setup. Replaced by the [quality-focused study](../designs/2026-09-14-quality-weighted-replay-study.md).

An isolated baseline continuation produced a complete 104-update chronological
corpus from the paused update-1744 parent. It can support the registered
fixed-arrival screen. It does not predict successes a changed learner would
have generated, and the child model is not adopted on main training.

## Coverage

Inspection of retained updates 1617–1744 found 18,658 success attempts and 14,562
missing attempt-to-source bindings. Ordinary events and the surviving pool could
not reconstruct that interval. No historical native rollouts were repeated.

The new corpus covers updates 1745–1848, using the actual generator/curriculum,
32 logical streams, 16 workers, CUDA behavior/learning and existing native limits.

| Recorded quantity | Count |
| --- | ---: |
| Initial active entries | 4,096 |
| Ordered terminal attempts | 14,735 |
| Admission events | 14,732 |
| Active duplicate events | 3 |
| Eviction events | 14,732 |
| Complete updates | 104 |

Admission counts are events, not a claim of globally distinct semantic IDs.
Every boundary reconciles ordered attempt IDs with admission/duplicate events.
Each attempt retains its terminal return, full labels, feature metadata and
packed policy inputs through a separately authenticated diagnostic row union.
Baseline publication coverage/bytes and union coverage/bytes are distinct.
Live credit and retained checkpoint roots are recorded at their actual boundaries.
The frozen parent checkpoint and all initial source dependencies are retained.

## Continuity and cost

The diagnostic uses the normal `run`, persistent workers, input registration,
`Learner.update`, publication, evaluation and pruning. Its observer is temporary,
absent from checkpoint state, and never enabled on ordinary execution. Existing
compact source files and actual input files are linked unchanged. Initial policy
rows are decoded by source once and reused for learner hydration.

Three focused release-native tests pass: observer enabled/disabled has identical
model, both Adam states, RNGs, buffer, events, selected IDs and forward counts;
one terminal can publish multiple successes with complete later return; duplicates
and re-admissions have separate attempt keys; and a same-update-evicted row remains
available after its baseline source is pruned. The combined declarative Python
suite passed 47 tests with one Linux-only test skipped locally. Rust workspace
check/tests passed in prerequisite work with unchanged Rust/Cargo sources.

The capture took **1,568.706 seconds** including setup and normal endpoint
finalization, below its 1,800-second cap. The normal learner duration was bounded
to 1,500 seconds with the existing 300-second finalization allowance; it stopped
at update 1848 rather than extending to the maximum 128 updates. New diagnostic
row/metadata writes used 631,130,400 bytes and 150.655 seconds. Peak total added
files, including new child artifacts and actual inputs, were 1,521,419,960 bytes,
below 2 GiB. New actual input files accounted for 528,561,819 bytes. Peak observed
process-tree RSS was 31,694,454,784 bytes, below the unchanged 48 GiB ceiling.
These diagnostic write costs must not be presented as normal-training overhead.

The child completed its endpoint evaluation and all workers exited. Main training
remains paused at update 1744, with identical launch/matrix/job bindings.

## Reproduction and retained evidence

Run from `/home/ashrimp/Srv/aic-retention-experiments-20260914` using the existing
Python environment under the `aic-ppo-padded-curriculum-12h-20260911` source:

```sh
python tools/factory-edit-pilot/diagnostics/declarative_success_arrivals.py inspect \
  --parent /home/ashrimp/Srv/aic-declarative-efficiency-20260913/artifacts/resumed-four-hour \
  --output artifacts/arrival-inspect
python tools/factory-edit-pilot/diagnostics/declarative_success_arrivals.py capture \
  --parent /home/ashrimp/Srv/aic-declarative-efficiency-20260913/artifacts/resumed-four-hour \
  --output artifacts/arrival-capture \
  --environment /home/ashrimp/Srv/aic-declarative-batch-20260913/.tracking.env \
  --updates 128 --seconds 1800
```

`artifacts/arrival-capture/corpus/manifest.json` binds the parent, frozen checkpoint,
initial pool, 104 boundary records, two source representations and native/runtime
source. Full evidence stays on the server; the compact manifest is also retained
locally at `artifacts/retention-experiments/20260914/arrival-manifest.json`.
These commands describe the completed bounded run; do not repeat it automatically.
