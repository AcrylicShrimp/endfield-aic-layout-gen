# One-hour native PPO trial

## Status

Stopped early at the user's explicit request, after 3,147.4 seconds
(52 minutes 27 seconds). The user identified the single-row/single-column
contraction action granularity as a critical issue. No automatic continuation.
The original one-hour deadline was superseded by this stop request.

## User decision and contract

The user requested a one-hour trial, followed by inspection, and explicitly chose
fresh initialization rather than importing an earlier checkpoint. Use the selected
`current-history-16` actor input, seed 17, eight persistent collectors and CUDA
FP32 with the existing PPO/GAE hyperparameters. No algorithm changes.

- Each training batch has 32 fresh factory attempts with a 32-action horizon.
- Allow at most 4,096 updates; stop the process after 3,600 wall-clock seconds even
  if the update schedule is unfinished. The frozen supervisor has a 46,860-second cumulative safety
  ceiling so a later user-approved continuation can reuse the same protocol;
  the outer timer is the authoritative one-hour stop. Bounded shutdown
  and terminal metric export follow the training window.
- Evaluate at initialization and every eight updates on the same 32 held-out
  generation seeds. Evaluation time counts against the hour. The last complete
  evaluation may precede the final learner checkpoint; report both positions.
- Preserve every learner checkpoint. Keep full training experiences for batches
  1, 2 and each eighth update, all evaluation evidence, and interrupted work.
- Enforce 32 GiB aggregate sampled RSS and a 16 GiB free-disk reserve. Use the
  existing W&B observer and explicit terminal export.
- After the hour, inspect admitted actions, evaluation area reduction relative
  to initialization, PPO KL/entropy/critic statistics, resource use, failures and
  final checkpoint identity. A one-seed trial is not evidence of convergence.

## Reproduction

Prepare a fresh run using `tools/factory-edit-pilot/ppo-one-hour-trial.json`.
Run the normal harness under GNU `timeout --foreground --signal=TERM
--kill-after=90s 3600`. The supervisor handles TERM as a stop request and retains
its last durable checkpoint and partial work. Do not silently resume or extend
this trial after the requested hour. The user explicitly requires joint result
review before deciding whether to run another twelve hours.

## Activity

- Record the user-selected fresh start and reuse the verified native-v2 harness.
- Raise only the existing configuration update-count ceiling from 256 to 4,096
  so a later approved continuation cannot exhaust the frozen schedule early.
  This does not authorize execution beyond the first one-hour timer.
- Configuration and minimal schedule-ceiling change committed as `db34894`.
  Existing 44 PPO tests, configuration/seed-disjointness validation, workspace
  check, and artifact worktree/index checks pass. Rust sources are unchanged
  from the preceding 541-test workspace run.
- Freeze an isolated runtime at `/home/ashrimp/Srv/aic-ppo-one-hour-20260910`.
  Run root: `artifacts/factory-edit-pilot/ppo-one-hour-trial` within that runtime.
  Protocol SHA-256: `33740e98524a0e6512165c8b25e7c0e2c8bf397e1f4841b425586935c9a4aba4`.
  The protocol records 256 source/binary/input identities. The ignored launch
  archive records the exact committed source; existing verified native binaries
  are copied without rebuilding. Credentials remain outside the runtime.
- Start one detached timed session. The initial evaluation admits all 32
  factories; the first training collection also admits all 32 and reaches the
  CUDA learner. W&B publishes the initial evaluation successfully.
- Live W&B run: <https://wandb.ai/acrylicshrimp-personal/endfield-aic-layout-gen/runs/357b97c90f050966c1b59dcc>.
  The ignored launch driver calls normal terminal export and writes
  `one-hour-summary.json` after shutdown, verifying durable record and final
  checkpoint hashes. It never resumes automatically.
- The user explicitly stopped training after confirming each contraction action
  removes exactly one row or column. Issued the normal CLI stop; both durable
  state and supervisor report `stopped` with `ppo-harness.user-stop`. No live
  owned training processes remain. Terminal W&B export completed successfully.
- Final committed state: 165 training batches, 165,696 admitted training actions.
  Final checkpoint: `current-history-16--seed-17/training-0165/checkpoint-3011322e84dd4966b0b8cd4284b544bf.pt`.
  SHA-256: `95f83c5b99c77dcfc66da0f1ec87fe64b8a15c73bbb7326aae78df0d98f64ecd`.
  Its actual file hash matches the durable manifest. Checkpoints, retained
  episodes and interrupted work remain preserved. The automatic terminal
  summary is available at `one-hour-summary.json` in the run root.
