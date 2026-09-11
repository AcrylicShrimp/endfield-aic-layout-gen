# Persistent blueprint PPO learning experiment

Status: complete.

## Context

The user authorized the next learning experiment after native blueprint-draft
verification. See [the registered contract](../designs/2026-09-11-blueprint-persistent-ppo.md).

## Completion criteria

- [x] Durable native draft snapshots with validated restoration.
- [x] Packed blueprint graph learning and nonterminal GAE bootstrapping.
- [x] Persistent streams, checkpointed optimizers/RNGs and continuation audit.
- [x] Readiness verification, then two registered learning runs to completion.
- [x] Timed evaluation, exact topology audit, report and selected evidence.
- [x] Workspace checks, artifact checks and focused commits.

## Activity

- 2026-09-11: Registered the 32-action chunk / 256-action episode experiment,
  four explicit training starts, two seeds and timed evaluation before coding.
- 2026-09-11: Implemented native snapshot restoration, persistent packed PPO and
  source-bound checkpoints. Readiness v2 completed two updates and six evaluation
  cases; the independent audit reconstructed both updates exactly, checked four
  continuing boundaries/eight native restorations, and validated six unique
  witnesses across 60 commit/final references. Forced KL stopping exercised the
  critic-head tail with the actor and its Adam state frozen.
- 2026-09-11: Committed the verified implementation as `96ab815`, then completed
  both registered seeds: 32,768 training actions, 128 real episodes and 48 timed
  evaluation cases. A separate CLI continuation from durable readiness update 1
  completed and passed its audit.
- 2026-09-11: Reconstructed all 128 main-run updates exactly, checked 896 live
  boundaries/nonterminal bootstraps and 1,024 native restorations, and validated
  all 426 unique retained witnesses. Preserved selected checkpoints and all
  non-replayable experience; removed regenerable intermediate optimizer states.
- 2026-09-11: Recorded the [results and research decision](../reports/2026-09-11-blueprint-persistent-ppo.md).
  Three sampled area improvements occurred beyond action 64, but aggregate final
  timed quality was lower than the untrained baseline in both seeds. Passing
  persistent-draft mechanics is not evidence of improved learned optimization.
