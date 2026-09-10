# Twelve-hour CNN-2 continuation launch

Status: launched; training is still running. This report verifies launch and
continuation, not completion of twelve hours or an improvement in final quality.

Continue the completed spatial comparison's two-layer actor from batch 32 for
43,200 additional supervised seconds. Restore actor, critic, both Adam optimizers,
shuffle RNG and Torch CPU/CUDA RNG. Keep graph depth two, width 32, horizon 32,
eight workers, CUDA FP32, route work 5,000 and the existing PPO/reward settings.
The 16,384-batch schedule provides headroom; the supervisor's cumulative duration
ends the run normally and preserves its last committed checkpoint.

Training uses newly generated factories with generation/action seed bases
211001024/221001024, following the parent's 1,024 attempted training starts.
The same 32 evaluation factories remain frozen, evaluated initially and every
eight new batches. New batch zero is parent batch 32. W&B records include both
new-run and cumulative batch/action counts. This reused evaluation cohort is a
progress diagnostic; it is not an independent generalization test.

## Verification

All 56 PPO Python tests passed, including strict parent identity, optimizer/RNG
transfer, consumed-seed overlap and evaluation-only frozen-input tests. Workspace
check and tests passed. The local native extension was rebuilt after its embedded
Cargo.lock identity failed the stale-build guard. No Rust behavior changed.

A separate CUDA readiness run completed two batches of eight fresh attempts.
Every initial model tensor, optimizer state and RNG state matched the parent.
All 32 evaluation outcome rows, excluding duration and output path, matched the
parent batch-32 evaluation. Mean best-area reduction remained
56.61992942656936%. The native audit validated 80 results, checked behavior
probabilities/values, and reproduced the second update exactly from its nonempty
optimizer checkpoint. Readiness weights are not used by the long run.

An online 20-second duration probe exited normally with child codes `[0, 0]`,
retained a committed checkpoint and finished W&B with
`termination_reason=duration-reached` and `training_complete=false`.

The runtime's 458 pilot source/configuration files match commit `1fcebac`.
The parent runtime, protocol, state and checkpoint remain unchanged. Historical
Python observation-oracle code is not part of the runtime import graph; the
continuation guard covers active model/execution sources and native binaries.

## Server ownership and timing

Runtime: `/home/ashrimp/Srv/aic-ppo-cnn2-12h-20260910` on `ashrimp-arch`.
Run: `artifacts/factory-edit-pilot/cnn2-twelve-hour` within that runtime.
Active user service: `aic-ppo-cnn2-12h-20260910-session4.service`.

Initial session began 2026-09-10 17:01:21 KST. The server initially had user
lingering disabled, so SSH logout stopped the first session after 10.77 seconds.
Its coordinator and observer exited with code zero; initial evaluation and partial
training inputs were preserved. Enable lingering for `ashrimp`, then resume the
same immutable run at 17:02:01 KST. The cumulative 43,200-second cap includes the
first session, including later tracking recovery sessions. Expected stop is approximately
**2026-09-11 05:02:32 KST** plus bounded finalization.

The first interrupted W&B initialization left remote run metadata incomplete.
Sessions two and three trained safely but their observers could not resume that
remote run (HTTP 404/internal errors, then a local SDK run-ID conflict). Stop
normally, initialize the same remote ID without resume once, verify its history
cursor is zero, and finish that metadata repair. Session four resumes both the
unchanged local run and repaired remote ID. It uploads the preserved records;
no training records were discarded or remote history overwritten. The previous
tracking diagnostic remains in tracking.json as historical text even though its
current status and remote state are running.

The launch audit observed 11 additional batches (43 cumulative), 11,104 new
training actions and 13 acknowledged W&B history rows. Source and checkpoint
hash checks passed while the service continued after multiple SSH disconnects.
Follow [the live W&B run](https://wandb.ai/acrylicshrimp-personal/endfield-aic-layout-gen/runs/d5669013d9f238418ebea67f).

The service has `Restart=no`; the supervisor owns workers and tracking cleanup.
With `Linger=yes`, the user service survives logout independently of this task.
A reboot or genuine failure can still interrupt training; no automatic extension
or restart is configured. Preserve interrupted inputs and checkpoints for explicit
recovery instead of assuming a seed can replay all wall-clock-limited work.

## Commands and retained evidence

```sh
cd /home/ashrimp/Srv/aic-ppo-cnn2-12h-20260910
PY=tools/factory-edit-pilot/.venv/bin/python
RUN=artifacts/factory-edit-pilot/cnn2-twelve-hour
ENV_FILE=/home/ashrimp/Srv/aic-ppo-area-20260909/.env
$PY tools/factory-edit-pilot/ppo_harness.py --env-file "$ENV_FILE" status --run "$RUN"
systemctl --user status aic-ppo-cnn2-12h-20260910-session4
# Request normal early termination, preserving committed work:
$PY tools/factory-edit-pilot/ppo_harness.py --env-file "$ENV_FILE" stop --run "$RUN"
# Only after an interruption, and only within the original cumulative duration:
$PY tools/factory-edit-pilot/ppo_harness.py --env-file "$ENV_FILE" resume --run "$RUN" \
  --duration-seconds 43200
```

The tracked configuration is `tools/factory-edit-pilot/ppo-cnn2-twelve-hour.json`.
Prepare it in a fresh output directory and use the same `run` command with
`--duration-seconds 43200` under a detached server service. It explicitly references
its parent at `/home/ashrimp/Srv/aic-ppo-spatial-20260910/artifacts/factory-edit-pilot/ppo-spatial-cnn2`.

Keep all checkpoints/evaluation records and selected training experiences under
ignored artifacts. Retain training batches 1/2/8/16/24/32 and every 128 thereafter;
the existing transactional cleanup removes other regenerable intermediates.
`artifacts/factory-edit-pilot/launch-evidence` retains readiness, state-transfer,
duration-stop and runtime-source audits. The adjacent compact provenance manifest
records final launch identities and live progress. Twelve-hour completion and
quality analysis remain future work after the session ends.
