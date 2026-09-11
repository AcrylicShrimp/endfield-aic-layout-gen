# Persistent blueprint PPO learning experiment

Status: in progress.

## Context

The user authorized the next learning experiment after native blueprint-draft
verification. See [the registered contract](../designs/2026-09-11-blueprint-persistent-ppo.md).

## Completion criteria

- [x] Durable native draft snapshots with validated restoration.
- [x] Packed blueprint graph learning and nonterminal GAE bootstrapping.
- [x] Persistent streams, checkpointed optimizers/RNGs and continuation audit.
- [ ] Readiness verification, then two registered learning runs to completion.
- [ ] Timed evaluation, exact topology audit, report and selected evidence.
- [ ] Workspace checks, artifact checks and focused commits.

## Activity

- 2026-09-11: Registered the 32-action chunk / 256-action episode experiment,
  four explicit training starts, two seeds and timed evaluation before coding.
- 2026-09-11: Implemented native snapshot restoration, persistent packed PPO and
  source-bound checkpoints. Readiness v2 completed two updates and six evaluation
  cases; the independent audit reconstructed both updates exactly, checked four
  continuing boundaries/eight native restorations, and validated six unique
  witnesses across 60 commit/final references. Forced KL stopping exercised the
  critic-head tail with the actor and its Adam state frozen.
