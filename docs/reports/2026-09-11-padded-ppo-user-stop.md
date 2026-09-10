# Padded PPO stopped before actor/critic experiments

The user requested termination of the twelve-hour run to test actor/critic
architecture and optimization choices. The public `stop` command completed;
the run is `stopped`, its systemd service is inactive with MainPID 0, and no CUDA
compute processes remain. The observer finished successfully and exported all
323 committed records with termination reason `user-stop`.

The run lasted approximately 66 minutes and committed 258 training batches,
520,160 training actions. An interrupted collection for batch 259 does not count
as training. Its worker signal diagnostic is subordinate to the supervisor's
explicit user-stop outcome. The run did not complete the planned twelve hours.

The last evaluation, at batch 256, covered the same 32 factories: mean best-area
reduction 63.8237%, versus 47.1164% initially, a paired gain of 16.7073 percentage
points. All 32 improved relative to initialization. Rejected actions were
141/1,024 (13.7695%); three exhausted routing work budgets. The first eight actions
contributed 60.3615 points and the remaining 24 contributed 3.4622 points.
These reused evaluation cases are diagnostic evidence, not an unseen final test.

## Retained checkpoint

Server root:
`/home/ashrimp/Srv/aic-ppo-padded-curriculum-12h-20260911/artifacts/factory-edit-pilot/padded-curriculum-twelve-hour`

- Checkpoint: `current-history-16--seed-17/training-0258/checkpoint-b3d780305fbf4ef49f695da0b4f23ca9.pt`
- Verified file SHA-256: `4880440115b004f749bebf5842efce1db6f7cedef48a6df62af3aa8d59f3c312`
- Actor SHA-256: `246fa0395dd19fda0dd38db688413ddd1c4550b8770fc0d0e74da6f5de8bbf23`
- Critic SHA-256: `9cd5f44160bf63e04b2a47ff462918858535724e5056f3bfd18c4442db68be7f`
- [W&B run](https://wandb.ai/acrylicshrimp-personal/endfield-aic-layout-gen/runs/86bdb8ba1b448abf0f4fc4cc)

Retained checkpoint, optimizer/RNG state, traces and frozen runtime remain in
their original ignored artifact locations. This work does not overwrite them.

## Follow-up

The user expanded the comparison to combinations of shared/separate encoders,
critic action history, and critic learning after policy KL stopping. Architecture
sharing and history availability must be separate factors. Any critic-only tail
in a shared model must preserve the actor, including its shared encoder weights.
