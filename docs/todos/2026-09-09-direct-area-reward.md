# Direct area reward milestone

## Status

Completed, audited and retained; committed with the implementation and report.

## Context

The user approved replacing cross-factory ranks with direct best-area reduction,
causal future credit and continuous matched-evaluation performance plots. Follow
[the contract](../designs/2026-09-09.42-direct-area-reward.md). The paused rank
continuation is retained; PPO, critic/GAE and secondary/dead-space rewards are
separate follow-up milestones.

## Completion criteria

- Implement per-action high-water area reward and undiscounted reward-to-go.
- Verify reward telescoping, restore/no-op behavior, causal gradient equivalence,
  payload rejection, physics/sampling parity and worker-independent updates.
- Run the declared two-seed comparison from matched untrained checkpoints.
- Audit witnesses, replay samples, optimizer continuity and checkpoint identities.
- Publish continuously refreshed evaluation plots with verified local mirrors.
- Record all outcomes and limitations, verify the repository and commit.

## Activity log

- Preserved the stopped rank learner and its complete short-pilot reference.
- Defined the first comparison to change reward/credit while holding the actor
  and optimizer settings fixed. No PPO or critic is silently introduced.
- Unit checks passed for exact telescoping, recovery without duplicate rewards,
  reward-to-go gradient equivalence, temporal causality, malformed payload
  rejection before mutation and matched evaluation identity.
- Linux serial/two-worker native parity passed, including identical samples,
  rewards, gradients and assembled Adam steps. The reduced learning/plot/mirror
  check completed before the full comparison.
- The user explicitly authorized the next experiment after this comparison:
  PPO-Clip with an actor, critic and GAE. Keep direct area reward and evaluation
  conditions. Initial proposal: clip epsilon 0.2, gamma 1 and GAE lambda 0.95.
- Full comparison completed all 64 updates and 64,416 training selections.
  Final matched changes were -0.212952/-0.888435 pp for seeds 17/29.
- Independent reopening audit checked 2,208 result records, reconstructed all
  64 Adam transitions and replayed 128 actions. Both live observers reached
  completed status with all ten evaluation measurements.
- Findings and reproduction commands are in the
  [completed report](../reports/2026-09-09.42-direct-area-reward.md). PPO remains
  the active next milestone, with the stopped rank continuation preserved.
