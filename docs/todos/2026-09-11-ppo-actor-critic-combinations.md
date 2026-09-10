# PPO actor/critic combination screen

Status: complete. Implementation is committed as `0d72382`; all experiments and
audits passed. See the [final report](../reports/2026-09-11-ppo-actor-critic-combinations.md).

Context: user requested stopping the long run and testing several combinations,
including critic history, rather than limiting the work to two alternatives.
The prior run is stopped and preserved; stop evidence is committed as `1531c92`.

Completion criteria:

- [x] Stop prior training and verify last checkpoint and tracking finalization.
- [x] Define factors, controls, optimizer ownership and predictive diagnostics.
- [x] Implement model, schedule and observation contracts with focused tests.
- [x] Complete native/CUDA readiness and workspace verification.
- [x] Freeze and execute all eight combinations with two initialization seeds.
- [x] Audit predictions, geometry matches, optimizer counts and retained inputs.
- [x] Report paired quality, critic errors, factor interactions and limitations.
- [x] Probe mature critics on identical held-out trajectories after exact replay.
- [x] Commit verified implementation and concise findings.

Activity log:

- 2026-09-11: Registered the full factorial design before implementation. Actor
  inputs are fixed; shared-encoder critic tails freeze all actor parameters.
  Fresh starts in this screen hold the factory input distribution independent
  of earlier policy improvements.
- 2026-09-11: All four shared/history readiness combinations completed two
  batches and 992 training actions each. Native audits verified 192 result
  records, behavior predictions and exact second-batch checkpoint continuation.
  Every audit exercised one actor epoch and four critic epochs while preserving
  actor parameters and optimizer state during the critic tail.
- 2026-09-11: Froze `0d72382` in a separate server runtime, verified 272 runtime
  sources against readiness, prepared all eight protocols and launched sequential
  two-seed cells. Learning settings remain fixed for the complete matrix.
- 2026-09-11: All 16 runs completed 512 batches and 1,025,024 training actions.
  The full audit checked 80,594 file identities, 4,416 matched input positions,
  all eight exact schedule prefixes and 496 native final layouts from 512
  attempts. W&B matched 672 remote rows and 65,047 numeric values.
- 2026-09-11: Replayed original batches 128, 192 and 256 exactly, then completed
  critic fitting with frozen actors. All 96 held-out episodes passed native and
  observation/reward validation. Predictive MAE improved at two snapshots and
  slightly worsened at the third; no downstream actor gain is claimed from this
  fixed-actor probe.
- 2026-09-11: Recorded every combination, seed-level effects, interactions,
  measured training work and limitations. Retained raw evidence outside Git and
  inspected all three figures. Final workspace checks/tests and 88 Python tests
  (85 passed, three platform-specific skips) completed successfully.
