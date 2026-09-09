# PPO, critic and GAE area-reward comparison

## Status

Completed at the amended 32/16 endpoints, with final evaluations, endpoint
verification and descriptive analysis. Results and retained provenance are
recorded in the slice report.

## Context

Use the same direct best-area reward, actor and evaluation banks. The critic
predicts remaining gains from current/best state and remaining budget. Preserve
valid temporary exploration and the incumbent witness. Follow the
[stage contract](../designs/2026-09-09.43-ppo-critic-gae-area-reward.md).

## Completion criteria

- Implement actual probability-ratio clipping, a separate critic, terminal-aware
  GAE, frozen behavior experience and checkpointed shuffled minibatch updates.
- Verify mathematical formulas, old-policy ratios, actor/critic separation and
  exact optimizer/shuffle continuity; pass real-native worker parity and smoke.
- Measure memory, observation payload size and update time before the full run.
- Run seed 17 through 32 batches and seed 29 through its batch-16 evaluation,
  preserving the original registration and explicit user budget amendment.
- Audit witnesses, experience, targets, checkpoints, optimizer transitions and
  declared physical/value observation replays; inspect delayed-gain trajectories.
- Continuously publish verified performance and ratio/critic diagnostics.
- Record complete results and limitations, verify the repository and commit.

The additional full offline optimizer reconstruction audit is cancelled under
the user's revised scope. Report existing online checks and retain the results
without repeating all training again.

## Activity log

- The full direct-reward comparison completed and passed its independent audit
  before PPO training began; its implementation and findings are committed as
  `df040ca`.
- Local/Linux math checks and real-native one/two-worker parity passed. The
  reduced run, all-behavior/GAE audit, two optimizer reconstructions and live
  performance/ratio/critic plot checks completed.
- The full two-seed comparison is running under frozen source hashes. A separate
  descriptive supplement checks behavior values against actual remaining
  returns and reports raw versus standardized delayed-action credit. It does
  not change learner settings or consume data for another optimizer update.
- Recorded the user-approved PPO + critic + GAE contract before implementation.
- Implemented a separate current/best-world critic while keeping the actor's
  action-only information and initial parameters unchanged.
- Unit checks pass for both clipping signs, hand-calculated GAE with delayed
  gains, masked/kind-balanced ratio one, critic-only updates, malformed behavior
  rejection and exact Adam/shuffle continuation with nonempty optimizer state.

- Both final evaluations improved all 16 factories versus the untrained policy;
  mean area reductions reached 53.35% and 55.17%. Finalization verifies the
  recorded online checks without another optimizer reconstruction.
