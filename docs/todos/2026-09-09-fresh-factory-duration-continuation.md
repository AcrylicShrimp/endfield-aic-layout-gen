# Fresh factory duration continuation

## Status

Paused during the user's algorithm clarification. Source-pilot and reduced
continuation audits passed, and the full run reached global update 50 before
its supervisor and workers stopped. Actual checkpoints and experience remain
retained. The user subsequently approved a separate direct-area reward study;
this rank continuation remains stopped and is not its initialization.

## Context

The user authorized leaving the fresh per-episode factory method running for a
longer period. Use the earlier twelve-hour duration, the measured 24-worker
configuration and seed 17's saved state. The seed was fixed before the second
short-pilot seed's result. Follow the
[contract](../designs/2026-09-09.40-fresh-factory-duration-continuation.md).

## Completion criteria

- Verify the completed source pilot and preserve actual policy/Adam/RNG state.
- Pass a reduced real-native continuation, independent optimizer/trajectory
  audit and detached process-cleanup check on the experimental server.
- Launch the full twelve-hour continuation with unchanged task distribution,
  batch size, horizon and learning settings, with 24 workers.
- Observe its resumed endpoint evaluation and first audited optimizer batch.
- Record source/checkpoint identities, server process location and progress;
  commit finished implementation and launch evidence without claiming that the
  twelve-hour training period has already completed.

## Activity log

- Defined the contract for twelve training hours plus a bounded final evaluation,
  periodic evaluation every 64 global updates, native/checkpoint auditing per
  batch and preservation of the actual generated experience stream.
- Implemented exact optimizer/RNG restoration, continuous episode cursors,
  compact append-only batch manifests and bounded constructor-output cleanup.
- Added a separate Linux supervisor with a hard wall limit and process birth
  identity checks, including cleanup of workers in separate sessions.
- Four tests pass on the server: exact next-update continuity after Adam/RNG
  restoration, input/failure preservation during cleanup, rejection of unrelated
  cleanup paths, and forced cleanup of detached workers while preserving an
  unrelated process.
- The source audit verified 2,592 result records, all optimizer updates and 496
  re-executed actions. The reduced continuation completed two updates, with all
  16 result records and four re-executed actions passing its independent audit.
- Launched seed 17 from update 32 at 2026-09-09 22:41:47 KST with 24 workers.
  Startup evaluation exactly matched the source endpoint.
- Paused after the user asked whether the learner was PPO. The current learner
  remains group-relative REINFORCE. Eighteen additional updates consumed 569
  admitted factories out of 576 attempts and 18,208 selected actions. The last
  checkpoint is `group-000049/checkpoint.pt`; all owned workers stopped.
