# Wider shared PPO model

Status: complete; model/configuration verified, training comparison not run.

The user requested increasing shared-model capacity to approximately the total
parameter count of the separate width-32 actor/critic pair (93,090). This slice
adds a runnable model configuration and verifies forward/backward, optimizer
ownership, critic-tail freezing and checkpoint continuation. It does not include
a new training comparison or claim a performance improvement.

## Contract

- Input: the existing PPO harness configuration with an explicit model schema
  `ppo-shared-capacity-v1`, width 40, critic width 40 and a shared actor/critic
  treatment. Critic history and critic-after-KL remain independent booleans.
- Output: the existing actor/critic pair, checkpoints and harness diagnostics.
  The shared world and history modules must retain object identity; optimizers
  must own every parameter exactly once.
- Keep the two CNN layers, two graph layers, four-group normalization, input
  contracts and PPO update semantics unchanged. Widen the coupled encoders and
  heads together rather than introducing independent width adapters.
- Preserve strict validation of the previous registered width-32 experiments.
  Reject separate encoders, other widths, changed depth or unexpected fields
  under the new schema with `ppo-spatial.model-spec` diagnostics.
- The supplied configuration uses critic history and stop-on-KL, with the same
  seeds, workloads and PPO settings as the completed factorial screen.
- Width-32 weights are not shape-compatible with width 40. Start fresh; the
  existing strict checkpoint loader must reject incompatible tensor shapes.

## Capacity choice

Four-group normalization requires a width divisible by four. Width 40 is the
nearest uniform choice: 86,282 unique parameters, 7.31% below the separate
reference and 47.41% above the old shared model (58,530). Width 44 would have
102,126 parameters, 9.71% above the reference. Width 40 has 75,241 actor and
61,281 critic parameters, of which 50,240 are shared and counted only once.

This approximately matches total storage capacity, not FLOPs or actor-only
capacity. The wider actor itself grows, so a subsequent experiment tests the
joint effect of sharing and width; equal parameter counts do not isolate gradient
interference. Keep both old shared and separate controls for such a comparison.

## Verification

- Enumerated shared widths 32, 36, 40, 44 and 48 using live model tensors;
  confirmed width 40 is the closest permitted uniform width to the reference.
- Extended the scalar/padded prediction and gradient comparisons, hidden-best
  isolation, history behavior, critic-tail freeze and exact checkpoint/update
  continuation checks to the wider shared model.
- Verified unique optimizer ownership, all four history/tail combinations,
  incompatible checkpoint rejection and strict model specification diagnostics.
- `python -m unittest discover -s tools/factory-edit-pilot -p 'test_ppo_*.py'`:
  89 tests, 86 passed and three existing platform-specific native skips on macOS.
- `cargo check --workspace` and `cargo test --workspace`: passed.
- Manually ran `ppo_harness.py prepare` using a local copy of the supplied
  configuration with the macOS native-library path, CPU execution and offline
  tracking. Output: `prepared`, two jobs and 37 phases per job. Model, workload
  and PPO settings were unchanged. No training process was launched.
- Raw preparation output and verification logs reside under ignored `target/`.

Activity log:

- 2026-09-11: Registered the width choice and scope before implementation.
- 2026-09-11: Added the versioned shared model and runnable screen configuration;
  completed model, update, checkpoint, CLI preparation and workspace checks.
