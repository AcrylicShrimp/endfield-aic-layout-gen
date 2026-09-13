# Four-hour declarative continuation preparation

Status: local preparation complete; server verification and launch explicitly held by the user.
Do not synchronize, verify on the server, schedule a start or run training until
the user explicitly resumes it. The selected duration is four additional hours.

## Registered intent and prerequisite findings

Continue seed 29, batch 1,024 from the completed comparison's update 32. Preserve
model, both Adam states, RNG, successful replay, episode cursors and the selected
next curriculum wave. Use one learner, 32 logical streams, eight physical workers,
minibatch 64 and the existing declarative/native/input/PPO/SIL settings.

Two existing boundaries need explicit preparation: the importer only accepts the
historical fresh-only v1 checkpoint, while the selected parent is curriculum v2;
the short-screen replay guard stops at 4,096 successes, while this parent already
contains 2,216. The user subsequently required replacement of the least needed experience at
capacity. The long configuration retains the existing 4,096-sequence / 262,144-
label limits and enables measured-utility replacement. Sampling among currently
retained identities remains uniform/balanced-available. Eviction removes entries
from the active replay pool and decoded cache; source chunks remain while any
retained checkpoint or active episode references them.

## Continuity checklist

| Preserved benefit / prior failure | New placement | Required verification |
| --- | --- | --- |
| Exact model, two Adam states and RNG; no fresh reset | Explicit v2 importer | Imported hash comparison; unchanged update from restored state |
| Actual generated requests/layouts and live draft identity | Original parent sources plus new input bank, continuing global slots | Missing/swapped input rejection; 32-stream live restoration |
| Strict-area 80/10/10 previous-wave starts | Copy selected v2 next-start plan without reselection | Exact plan/stream comparison and native inherited admission |
| Full terminal SIL credit, bounded replacement explicitly authorized | Existing buffer with observed-utility retention | Same IDs/labels/transition below capacity; correct replacement and source retention at capacity |
| Packed sampling, GPU update and prefetch; bounded memory | Existing pool/update kernel | Isolated release-native/CUDA timing before launch |
| Graceful time/signal stop and durable recovery | Complete episode-boundary stop, bounded finalization, exclusive run lock | Boundary/signal/time tests; integrated stop and restore |
| Valid best, separate evaluation and best-model retention | Existing panel and evaluation, explicit best checkpoint pointers | Coverage/validity and deterministic score ordering |
| Limits, artifacts, no normal proof/profiling | Existing lightweight guards and publication ledger | Configuration checks; bounded launch receipt and no diagnostic process |

Input: completed source-bound v2 parent, explicit long config, release library,
frozen evaluation panel and an isolated verification receipt. Output: a new
launch and imported checkpoint, immutable records and inputs, latest recovery and
best evaluated checkpoints, structured stop reason and compact final report.
Do not modify or reinterpret the parent launch/checkpoint.

The four-hour clock includes normal collection, updates, publication and routine
evaluation, excludes the parent's training time and isolated readiness, and
survives process restoration. Stop at a complete 64-action episode wave after
the target; allow a separately bounded five-minute finalization grace. A hard
limit failure retains the latest published checkpoint and worker partial evidence.
A requested stop pauses instead of claiming duration completion.

Final Linux/native/CUDA integration and performance closure are pending while the
server is reserved. Local checks may establish preparation correctness; they do
not authorize launch or stand in for the actual server launch receipt.


## Prepared configuration and verification boundary

`tools/factory-edit-pilot/declarative-four-hour.json` selects seed 29 from update
32, 32 logical streams on eight workers, batch 1,024 and minibatch 64. The target
is 14,400 additional seconds, with 300 seconds of arm finalization and a separate
60-second matrix startup margin. The very loose update ceiling is an operational
guard, not the training target. Evaluate every 32 additional updates and at the
actual endpoint; preserve extra evaluation boundaries from requested pauses.

The replay limits remain 4,096 sequences and 262,144 labels. The
[replacement rule](2026-09-13-declarative-replay-retention.md) is enabled explicitly.
Retain the latest two recovery checkpoints, the imported initial checkpoint and
best evaluated overall/small/large checkpoints. Select on the fixed full panel,
with mean area reduction first, then lower mean transport tiles and turns. Report
training-geometry exclusions separately; changing exclusions cannot change the
model-selection cohort. A selected checkpoint keeps its replay source chunks even
when those entries have since been evicted from the active buffer.

RSS is capped at 48 GiB on the previously observed 64 GiB server; decoded replay
can grow toward its fixed capacity. Retained learner outputs are capped at 32 GiB,
with a 16 GiB filesystem reserve also protecting new generated-input stores.
These are operational ceilings, not memory or throughput guarantees. Input stores
and immutable parent evidence remain bound to their original actual payloads.

Local validation passed: six retention tests, six duration/import/selection tests,
fifteen existing learner/batch/curriculum tests with the local release-native
library, `cargo check --workspace` and `cargo test --workspace`. Utility observation
preserves model/Adam/shuffle results and forward counts. The configuration CLI
reports seed 29, batch 1,024 and 14,400 seconds without opening any worker.

The explicit server readiness command runs the prepared normal CLI for two new
updates on a separate diagnostic input stream, checks automatic duration stop and
reporting, and restores the intermediate 32 unfinished native streams. It also
runs the targeted tests and records actual phase timings/RSS. Its receipt binds
config, source, parent, panel and worker allocation. Normal preparation rejects a
missing or mismatched receipt. This server-only step has NOT been run during the
hold, so the integrated continuity/performance checklist remains open there.

## Manual server procedure after the user lifts the hold

First create an isolated server checkout of this prepared source. Do not update
the frozen batch-comparison checkout. Reuse its unchanged release library and
tracking environment. The following commands are documentation only: none were
executed or queued during local preparation. Use the established Python venv.

```sh
aic_parent=/home/ashrimp/Srv/aic-declarative-batch-20260913/artifacts/declarative-batch/comparison-v1/seed-29-batch-1024
aic_native=/home/ashrimp/Srv/aic-declarative-batch-20260913/target/release/libaic_pilot_native.so
aic_panel=/home/ashrimp/Srv/aic-declarative-learning-short-20260913/artifacts/declarative-learning/panel-v1/panel.json
aic_environment=/home/ashrimp/Srv/aic-declarative-batch-20260913/.tracking.env
python tools/factory-edit-pilot/declarative_duration.py verify \
  --parent "$aic_parent" --native-library "$aic_native" --panel "$aic_panel" \
  --environment "$aic_environment" --output artifacts/declarative-duration/readiness
python tools/factory-edit-pilot/declarative_duration.py prepare \
  --parent "$aic_parent" --native-library "$aic_native" --panel "$aic_panel" \
  --verification artifacts/declarative-duration/readiness/verification.json \
  --output artifacts/declarative-duration/four-hour
nohup python tools/factory-edit-pilot/declarative_duration.py run \
  --output artifacts/declarative-duration/four-hour --environment "$aic_environment" \
  > artifacts/declarative-duration/four-hour/run.log 2>&1 < /dev/null &
```

Before the final command, confirm the isolated readiness process has ended and
review its actual normal timings/resource receipt. This is not an automatic
launch chain. The run command holds an exclusive file lock, handles SIGINT/SIGTERM
as a pause at a complete episode boundary and writes a compact report on successful
completion. Repeating `run` resumes a paused/failed run from its published state
and remaining time; a completed run is rejected. Logs and state remain local to
the new ignored artifact directory. There is no recurring monitor, scheduled
start, profiler, full-buffer inference or automatic completion replay audit.
