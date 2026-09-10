# PPO actor/critic combination screen

Status: implementing the registered eight-cell factorial experiment.

Context: user requested stopping the long run and testing several combinations,
including critic history, rather than limiting the work to two alternatives.
The prior run is stopped and preserved; stop evidence is committed as `1531c92`.

Completion criteria:

- [x] Stop prior training and verify last checkpoint and tracking finalization.
- [x] Define factors, controls, optimizer ownership and predictive diagnostics.
- [x] Implement model, schedule and observation contracts with focused tests.
- [x] Complete native/CUDA readiness and workspace verification.
- [ ] Freeze and execute all eight combinations with two initialization seeds.
- [ ] Audit predictions, geometry matches, optimizer counts and retained inputs.
- [ ] Report paired quality, critic errors, factor interactions and limitations.
- [ ] Commit verified implementation and concise findings.

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
