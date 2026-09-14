# Masked cold training: server efficiency and 12-hour launch

The new masked learner starts from random initialization after server profiling,
storage/packing optimization and a real pause/restore check. Normal training
measured **5.588 seconds/update, 183.246 actions/second, 2.082 occupied CPU cores
out of 24, and 30.411% GPU utilization** in one shared measurement window.
The run is online in [W&B](https://wandb.ai/acrylicshrimp-personal/endfield-aic-layout-gen/runs/09f92i5j).
These are execution results; no new learning-quality conclusion is made here.

## Changes and same-data evidence

The completed cold-mask study exposed expensive storage and CPU packing. A
representative 1,003-row replay archive contained 29,355,463 bytes. Python profiling
showed tens of millions of redundant Torch persistent-storage dispatches around
an already optimized tensor reducer. Normal generation also materialized repeated
request/layout JSON files, and state publication rewrote the full input history.

| Identical 1,003-row source | Save | Load | Archive size |
| --- | ---: | ---: | ---: |
| Previous Torch wrapper | 4.555 s | 1.725 s | 29.36 MB |
| Direct stream, same full payload | 1.904 s | 1.653 s | 29.39 MB |
| Direct stream, required replay branches | 0.605 s | 0.761 s | 19.13 MB |

This is one unprofiled pass per encoding on the same retained source. Full-payload
roundtrip values and all 1,003 recorded root/conditional likelihood inputs matched
exactly. Temporary comparison archives were deleted. Gzip level 1 remains: the
large cost was unnecessary object dispatch and stored content, not a reason to
add a new compression dependency or reconstruct objects before serialization.

Candidate kind normalization now moves to immutable worker preparation. Three
alternating passes over identical 1,003-row data took approximately **0.184 s
before and 0.0261 s after** (7.05x packing speed); one-time feature preparation
took 0.112 s. Every packed tensor matched exactly. Recorded conditional port
features are reused across PPO epochs and successful replay draws.

The resulting workflow is simpler:

- One direct compressed stream replaces the redundant Torch archive wrapper;
  the loader still recognizes existing archives.
- Replay retains the entire numerical root domain and only the recorded port
  branch, plus required unfinished credit and witness identity.
- Fresh input publication retains one actual request/layout archive and compact
  attempt metadata. Native construction/randomization passes values in memory,
  without successful-run setup, wiring stdout or duplicate layout exports.
- The native JSON entry point uses the same `Episode::new` validation. Newly
  generated validated values return directly; restored/external values are
  authenticated at their boundary. Stable catalog/store checks are reused.
- Recovery state keeps current input metadata, a slot frontier and a count.
  Existing update records own historical metadata once; explicit reports read it.
- Ordinary evaluation avoids writing then deleting full snapshots. Completed
  waves assign their next starts before taking their one durable worker snapshot.

The archive magic is `AICPKL2\0`; replay, actual-input and normal-state contracts
are v2. Existing input stores cannot be silently rebound to changed generator
sources. Legacy input/archive readers and explicit panel exports remain at their
format boundaries; there is no alternative normal learner implementation.

## Integrated measurement and remaining capacity

The final normal CLI used 32 logical streams, 16 persistent workers, CUDA behavior
and PPO, 1,024 actions/update and the original masked learning configuration.
It paused at update 2, restored the saved state and completed update 8. The final
probe had 128 registered inputs, both fresh and inherited starts, and 252 own
successful replay entries. It retained next-start assignments and did not recycle
input slots or use reserved evaluation geometries.

The five-update measurement spans update 3 publication through update 8
publication: 27.941 seconds, excluding startup and final evaluation. CPU and GPU
were sampled every 500 ms by an external bounded diagnostic; 55 CPU and 56 GPU
samples fell in this exact window. The CPU figure includes the trainer and worker
process tree; 2.082 cores correspond to 8.674% of the server's 24 physical cores.
Average publication time over updates 3–8 was 0.280 seconds. The eight-update
probe, including its evaluation and pause/restore work, accounted for 67.998
run seconds and peaked at 21,447,974,912 process-tree RSS bytes.

Earlier mature four-update controls measured publication at 4.590 and 8.429
seconds on nonterminal updates, versus 0.710 and 0.890 seconds after the initial
storage cutover. These were useful integrated storage checks. They are not an
exact overall learner speed ratio: elapsed-time observation features can change
later actions, successful replay and actor early stopping. The final cold probe
also has less replay than a mature long run; its speed is not a 12-hour guarantee.

GPU utilization is still below saturation. The process profile places remaining
cost in collection/transport, required packing and forward/backward work. The
existing one-batch-ahead preparation and persistent workers remain. A bounded
reuse of the existing `CompiledActor` on real joint likelihood batches passed
output and gradient tolerances (maximum output difference 2.38e-6), but 30
forward/backward steps improved only from 0.2666 to 0.2605 seconds. Warmup cost
7.42 seconds. That small microbenchmark gain does not justify a new normal
compilation path. No larger batch, changed optimizer schedule, extra CPU work or
altered environment workload is introduced to inflate utilization.

## Storage and recovery envelope

The final readiness bank used 1,464,280 bytes in 490 files including store/slot
metadata. Its eight update records used 620,435 bytes, seven replay sources
47,346,181 bytes, and five checkpoints 2,578,156 bytes. The readiness pause and
completion deliberately retain full final evaluation outputs (66,291,116 bytes);
the long run does not produce these at ordinary evaluation intervals.

As a mature reference, the unchanged older run at update 1908 held 4,096 replay
entries/4,614 labels, 777,547,598 replay bytes and 2,254,468,649 arm-ledger bytes
under the previous storage path. At the short measured speed, 12 hours would
allow roughly 7,700 updates. Extrapolating current input, metric and curriculum
costs and allowing room for mature replay/protected checkpoints gives an 8 GiB
planning envelope. This is an estimate, not a promised update count or hard
storage bound. Actual input files are included in that estimate even though the
arm ledger tracks arm-owned files separately.

The launch keeps a 16 GiB arm-output ceiling, 16 GiB free-disk reserve and 48 GiB
process-tree RSS ceiling. Available disk immediately before launch was about
73.0 GB. Each update remains recoverable; latest two and selected best all/small/
large checkpoints protect their required replay sources. No bulk trajectory,
routine historical audit, profiler or external utilization monitor runs in the
12-hour process. Required actual factory inputs and selected valid outputs stay
available for recovery and inspection.

## Verification and launch

Verification passed: `cargo fmt --all`, `cargo check --workspace`,
`cargo test --workspace`, 71 declarative tests, and 24 related execution/PPO/input
tests. Two constructor-gated tests skipped in the latter command were then run
explicitly with `AIC_RANDOM_INPUT_TESTS=1`; both passed. The server native module
was built against the actual Python 3.12 training environment. Checks include
legacy/new archive roundtrips, sparse unfinished credit, cached branch reuse,
fixed-input rejection, in-memory/file native parity and actual worker recovery.

Server root: `/home/ashrimp/Srv/aic-masked-long-opt-20260914`.
Normal output: `artifacts/twelve-hour`. Initial process: **107932**.
The launch uses seed **149**, generation seed **722900000**, **43,200 seconds**
plus a **600-second complete-wave grace**, and 128-update evaluation cadence.
All 128 evaluation geometries remain reserved; the 32-case development panel is
unchanged. Cold-origin proof records empty Adam/replay/history and no learned
state import. Reused fresh factory inputs do not include probe-trained weights
or inherited curriculum layouts. The older main remains paused at update 1908.

The [compact provenance manifest](2026-09-14-masked-long-training-efficiency.provenance.json)
binds configuration, sources, native binary, checks, measurements and cold origin.
Detailed local evidence is under `artifacts/masked-long-opt-20260914`; server
diagnostic evidence stays under the isolated root's `artifacts/`.

Reproduction from that server root uses the checked-in operation:

```sh
tools/factory-edit-pilot/.venv/bin/python \
  tools/factory-edit-pilot/operations/masked_long_training.py prepare-probe \
  --output artifacts/readiness-reproduction \
  --panel /home/ashrimp/Srv/aic-band-mask-training-20260914/artifacts/panels/development/panel.json \
  --reservation artifacts/reservation.json \
  --native-library target/release/libaic_pilot_native.so

tools/factory-edit-pilot/.venv/bin/python \
  tools/factory-edit-pilot/operations/masked_long_training.py run \
  --output artifacts/readiness-reproduction \
  --environment /home/ashrimp/Srv/aic-declarative-batch-20260913/.tracking.env \
  --stop-after 2
# Resume the same command without --stop-after. The probe ends at update 8.
```

`diagnostics/masked_cold_profile.py` wraps only a registered probe for optional
function profiling and bounded CPU/GPU telemetry. `masked_storage_compare.py`,
`masked_pack_compare.py` and `masked_compile_compare.py` reproduce the independent
same-input comparisons. None is called by normal training or on its completion.
