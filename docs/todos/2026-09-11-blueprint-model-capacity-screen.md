# Compare blueprint graph capacity

Status: complete.

Context: the user requested trying model changes after the harder blueprint
contract, then requested stopping monitored training and using the server.

Completion criteria:
- Preserve the stopped trainer, its final tracking acknowledgement and tail audit.
- Parameterize message rounds with exact default-baseline parity.
- Verify width/depth, batching, gradients and checkpoint restoration.
- Run four width/depth architectures with two seeds under the registered budget.
- Verify shared W&B metrics, child completion and final audits.
- Analyze area quality and compute cost; retain compact provenance and commit.

Activity log:
- 2026-09-11: Mac duration training paused at 1396 batches; W&B finished.
  The remote padded-curriculum run was already stopped and server was idle.
- 2026-09-11: Registered width 40/80 by two/four rounds, seeds 17/29,
  512 batches per run, fixed 32-case evaluation every four batches.

- 2026-09-11: Baseline initialization matches the stopped run exactly. Four
  capacity regressions, all 413 Python tests (seven skipped), workspace checks
  and Rust tests passed. Actual matrix CLI prepared all eight local arms.
  Server readiness and full comparison remain pending.
- 2026-09-11: All eight server readiness arms completed eight updates and passed
  their tail audits. The width-80/four-round full replay passed all eight updates,
  32 training episodes and 96 evaluation cases. All 768 readiness cases reached
  64 actions. Measured main-output estimate is 36.84 GiB, leaving 17.11 GiB free.
- 2026-09-11: Started the registered 512-update matrix under a supervised server
  user service on CPU slots 0, 1, 4 and 5. Created and read back a dedicated W&B
  view, including authentic iteration-zero evaluation points. Final results pending.
- 2026-09-11: All eight arms completed 512 batches and passed their final audits.
  All 33,024 evaluation cases reached 64 actions; the server service exited zero.
  Final two-seed area means were 0.9762%, 0.1491%, 0% and 0% in registered model
  order. The baseline had the highest aggregate quality, but ranking reversed
  between seeds for width 80/two rounds. No consistent scaling gain was found.
- 2026-09-11: Retained exact native binaries outside Cargo cleanup, complete
  server evidence, local metadata/curves and verification logs. Removed the
  temporary tracking environment after all observers finished. Final W&B
  verification compares every evaluation value and axis against durable records.
  Findings and limitations are recorded in the capacity screen report.
