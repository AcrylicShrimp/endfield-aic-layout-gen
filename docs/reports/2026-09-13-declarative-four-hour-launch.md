# Four-hour declarative continuation launch

Status: running; this records readiness and launch, not final learning results.

The user resumed server work after the explicit preparation-only hold. One
seed-29 learner, batch 1,024, continued from batch-comparison update 32 at
2026-09-13 19:44:41 KST (10:44:41 UTC). The target is four additional hours of
normal execution, approximately 23:45 KST, followed by at most the configured
five-minute finalization allowance. Complete 64-action episode waves remain the
stop boundary. Initial cache hydration and routine evaluation count toward time.

Source commit: `111d01c`. Server: `ashrimp-arch`. Isolated checkout:
`/home/ashrimp/Srv/aic-declarative-four-hour-20260913`. Run directory:
`artifacts/declarative-duration/four-hour`, relative to that checkout. Initial
coordinator PID: `1845`; use the saved state and command identity when inspecting
later, because a PID alone is not durable identity.

[W&B run](https://wandb.ai/acrylicshrimp-personal/endfield-aic-layout-gen/runs/mltpnnfq).
[Compact immutable provenance](2026-09-13-declarative-four-hour-provenance.json).

## Server readiness and continuity closure

The server initially had approximately 60 GiB available RAM, 113 GiB disk space
and an idle RTX 5090. The isolated checkout reused the unchanged native library,
CLI binary and Python environment. A first readiness attempt stopped before
training because the isolated checkout lacked `target/release/aic-cli`; copying
the unchanged predecessor binary resolved that setup failure. No model or
training implementation changed during launch.

The successful `readiness-v2` receipt completed in 262.747 seconds:

| Requirement | Evidence |
| --- | --- |
| Exact parent continuation | Model, both Adam states, both RNG states, replay and all streams preserved by hash; original selected wave 17 copied unchanged |
| Native behavior and local controls | All 12 targeted retention/duration/import tests passed on the server native library |
| Actual CUDA execution | Two new normal-command updates, 2,048 actions, 32 completed episodes |
| Actual input binding and recovery | Separate diagnostic fresh input stream; restored all 32 intermediate native streams at action 32 and compared saved bindings/cursors |
| Episode-boundary duration completion | Diagnostic duration stopped at update 34; compact final report completed automatically |
| Valid fixed-panel evaluation | Final report checked complete 32-factory, two-replicate evaluation coverage and validated exports |
| Preserved normal execution | Eight physical workers; no profiler, full-buffer inference, optimizer replay audit or extra diagnostic process attached to main training |
| Resource controls and startup cost | Peak normal-command process-tree RSS 19,287,781,376 bytes (17.96 GiB); first update at 206.365 s, second at 244.812 s |

The measured phase totals across those two updates were collection 30.781 s,
learner 8.128 s, publication 20.371 s and final evaluation 8.975 s. Worker
minibatch preparation 2.418 s overlaps learner work. These phase totals omit
cache hydration and some bookkeeping/startup costs; the first-update elapsed
time captures the substantial restoration cost. The second update took 38.447 s
of additional wall time. This short readiness is not a prediction of long-run
throughput, memory at full replay capacity or learning quality.

All readiness processes ended before preparing and starting the main run. The
main importer reread the original update-32 parent; diagnostic updates 33–34 are
not training history for the main run. The main launch retained the registered
fresh-input seed stream and online tracking, and its receipt matched source,
configuration, native binary, parent, panel and worker allocation.

Main update 33 was published at 205.870 s with 2,216 eligible replay experiences
and all four actor epochs. The coordinator remained active afterward. This
confirms the actual online launch advanced and saved recovery state; it is not
an evaluation result or evidence of long-run completion.

## Active run contract

The parent contributes 512 actual input records and 2,216 active successful
experiences. New global input slots start at 512; the already-selected next-wave
starts retain their actual original requests/layouts. The replay pool stays
bounded at 4,096 experiences / 262,144 labels and uses the approved measured
utility replacement policy. Routine evaluation is every 32 additional updates
and at the endpoint. Preserve initial, latest two recovery and best evaluated
overall/small/large checkpoints and all source chunks they require.

The normal limits are 48 GiB process-tree RSS, 32 GiB retained learner outputs
and a 16 GiB free-disk reserve. No recurring monitor or diagnostic workload was
added. The detached normal command owns an exclusive run lock. A SIGTERM/SIGINT
requests a pause at a complete episode boundary; resuming the same command uses
the saved learner and remaining elapsed-time budget.

Raw readiness and run artifacts remain under ignored server `artifacts/`; they
are not committed or portable recovery bundles. Main state, checkpoints and
logs change while training runs, so the provenance manifest records immutable
launch/import/readiness evidence rather than pretending to freeze active state.
