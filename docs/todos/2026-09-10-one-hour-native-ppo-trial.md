# One-hour native PPO trial

## Status

Prepared configuration; execution not yet started.

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
