# Short masked batch scaling benchmark

Status: complete; no replacement long run has been started. The user stopped the preceding 12-hour run to prioritize new
environment samples per minute over iterations per second. That run stopped
cleanly at update 174 (1563.92 seconds) and is retired, with its outputs retained.

Compare 32/64/128/256 logical environments with PPO minibatches 64/128/256/512.
Each cold arm runs four updates: two warmup updates and two measured updates.
Keep 16 physical workers, 32 actions per chunk, 64 actions per episode, four PPO
epochs, mask, learning rate, retention, curriculum and native budgets unchanged.
The measured window starts at update 2 publication and ends at update 4
publication, including normal persistence. Count new environment transitions;
repeated replay draws are not new samples. Record startup/evaluation-inclusive
throughput separately. This short experiment does not establish learning quality
or mature-replay throughput; different batch sizes consume different sample counts.

Reuse the canonical input adapter so overlapping global slots resolve to the same
actual factory payload, independent of logical stream count. Use one shared input
bank with seed base 722900000 and cold model seed 149. Evaluation retains the
existing 32-case panel and all 128 geometry reservations. No learned state from
the retired run initializes an arm. External 500 ms CPU/GPU telemetry stops with
each bounded arm; no profiler or bulk trajectory capture is enabled.

| Existing benefit | Reuse and verification |
| --- | --- |
| Persistent workers and compact shared-memory transport | Keep 16 workers and the existing pool; verify actual execution for each size |
| Cached immutable features and replay hydration | Use the unchanged optimized learner and direct compressed archive format |
| Canonical actual inputs and episode identity | Reuse `CanonicalInputs`, its slot/restore tests and shared store |
| Complete-wave recovery and native validity | Generalize positive stream-count validation; keep 32/64 action boundaries and final native-valid evaluation |
| Resource controls | Keep 48 GiB process-tree RSS and 16 GiB free-disk reserve; stop the sweep on a failed arm |

Only the artificial 8/32 stream validation restriction and probe configuration
plumbing change. No new CPU optimization precedes this measurement. The repository
server root is `/home/ashrimp/Srv/aic-batch-scale-20260914`; results are under
`artifacts/scale`. The short sweep uses
`tools/factory-edit-pilot/diagnostics/masked_batch_scale.py prepare` followed by
`run`; each arm uses the normal masked runner through the existing telemetry
wrapper. The long-run launcher still rejects custom configurations outside probes.

The user also requires a compiled-model comparison. After the eager sweep, test
the highest-throughput stream count with compiled learner and behavior root/port
heads. Reuse the existing dynamic/fullgraph compiler and eager parameter ownership;
keep CPU sampling order, optimizer state and checkpoint keys unchanged. A bounded
diagnostic checks first-use head outputs and gradients against eager computation,
records compilation events separately, and reports any compilation inside the
measurement window. No silent eager fallback is permitted. Compiler callbacks
observe shared global events, so their overlapping times must not be summed.

## Eager sweep results

| Environments | PPO minibatch | New samples/min | Relative | Seconds/update | CPU cores | GPU utilization | Peak GPU MiB |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 32 | 64 | 10,491 | 1.000x | 5.86 | 2.44 | 24.4% | 1,704 |
| 64 | 128 | 10,357 | 0.987x | 11.86 | 2.04 | 17.5% | 2,230 |
| 128 | 256 | 12,210 | 1.164x | 20.13 | 2.95 | 14.0% | 2,920 |
| 256 | 512 | 11,599 | 1.106x | 42.37 | 3.10 | 21.9% | 5,366 |

All four arms completed both episode waves and their final factories passed native
validation. The measured counts were 2,048/4,096/8,192/16,384 new transitions.
Peak process-tree RSS ranged from 21.55 GB to 34.73 GB, below the 48 GiB guard.
The 128-environment candidate leads this short sweep by 16.4%; 256 does not improve
on it. This is a single short screen, not a statistically established optimum.
The shared fresh-input slots are identical when they overlap; larger arms also
cover additional factories, and curriculum choices/actor early stopping differ.

Large batches did not saturate the GPU. Collection time grew with stream count,
and the larger minibatches did not yield proportional optimization throughput.
The result supports measuring batch size before more CPU work, but does not
support extrapolating an eightfold sample-rate gain from eightfold batch size.

For compilation, freeze all 256 possible fresh-input slots for the first two
128-environment waves, then run an eager control and a compiled arm against that
same existing bank. This bounded additional control is necessary because the
first sweep included newly generated inputs. Its cached-input rate must not be
presented as an apples-to-apples improvement over the first sweep's rate.

## Compiled model comparison

The matched cached-input 128-environment control reached **14,323 new samples/min**;
the compiled model reached **11,465/min**, 20.0% lower in this short window. Mean
GPU utilization rose from 20.0% to 28.3%, illustrating why sample throughput is the
primary metric. Both completed four updates and all final evaluation factories
were native-valid. Compiled learner root/port heads passed both gradient-enabled
and inference output checks; behavior heads passed inference checks. Parameter
and input gradients were checked on independent graphs, preserving the compiler's
normal buffer donation. There is no per-batch diagnostic after those first checks.

Seven compiler events totalled 14.53 seconds: 11.64 seconds in update 1 and 2.89
seconds in measured updates 3–4. Shared callback copies are counted once. The
compiled actor ran 3 and 4 actor epochs in the measured updates; the matched
eager control ran 1 and 3. Four critic epochs still ran in every update. The
short result therefore includes compiler specialization and different learning
work, and is not an isolated kernel speed comparison or proof about fully warmed
long-run compilation. The model sees elapsed time, so JIT timing and small
floating-point differences can change subsequent trajectories and early stopping.

An initial diagnostic attempted to retain an AOT graph for gradient comparison,
which conflicts with donated buffers. It failed before a training update. The
fixed diagnostic checks independent graphs, then runs the normal one-use backward;
the successful retry is `paired-compiled-v2`. The completed eager control was not
repeated. The retry also had no idle preparation process holding a CUDA context,
so peak GPU memory from the paired runs is not used to infer a compiler memory
saving. The initial failed result remains retained separately.

## Decision and verification

Use **128 environments / PPO minibatch 256** as the next candidate, based on this
short sample-throughput screen. Larger batches have not demonstrated a large
multiplicative gain, and the tested compilation configuration has not demonstrated
an integrated throughput gain. Do not treat these observations as a learning-
quality result or a global optimum. The preceding 12-hour run remains retired at
update 174; this task does not launch a new long run.

Verification: 72 declarative tests passed on the release-native server, including
canonical-slot identity and variable-count complete-wave validation. Local
`cargo check --workspace` and `cargo test --workspace` passed. The real normal-run
CLI completed all four eager sizes, the matched eager control and the compiled
retry. No source/model worker fallback or full trajectory archive was used.
All benchmark workers, compiler workers and external utilization samplers are
stopped after the measurements. Required actual inputs, compact update records,
checkpoints and summaries are retained under the isolated server root.

Reproduction after preparing the registered 32-case panel/reservation inputs:

```sh
# Run from /home/ashrimp/Srv/aic-batch-scale-20260914.
tools/factory-edit-pilot/.venv/bin/python \
  tools/factory-edit-pilot/diagnostics/masked_batch_scale.py prepare \
  --output artifacts/scale-reproduction \
  --panel /home/ashrimp/Srv/aic-band-mask-training-20260914/artifacts/panels/development/panel.json \
  --reservation artifacts/reservation.json \
  --native-library target/release/libaic_pilot_native.so

tools/factory-edit-pilot/.venv/bin/python \
  tools/factory-edit-pilot/diagnostics/masked_batch_scale.py run \
  --output artifacts/scale-reproduction \
  --environment /home/ashrimp/Srv/aic-declarative-batch-20260913/.tracking.env

tools/factory-edit-pilot/.venv/bin/python \
  tools/factory-edit-pilot/diagnostics/masked_batch_scale.py compile-pair \
  --output artifacts/scale-reproduction \
  --environment /home/ashrimp/Srv/aic-declarative-batch-20260913/.tracking.env
```

Compact results and source bindings are in the
[provenance manifest](2026-09-14-masked-batch-scaling.provenance.json).
