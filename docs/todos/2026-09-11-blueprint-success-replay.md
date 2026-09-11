# Accumulating policy-discovered success replay

Status: in progress.

Context: the user wants rare successful policy experience to remain available for
later learning, using an auxiliary imitation term beside PPO.

Completion criteria:
- Implement a durable, initially empty, independently auditable success buffer.
- Verify selection, continuation, replay timing, gradients and actual CLI resume.
- Run both registered two-seed arms without external teacher or evaluation labels.
- Replay every learner/buffer update and validate native witnesses.
- Report cold-start, reuse and autonomous quality outcomes; verify and commit.

Activity log:
- 2026-09-11: Registered original/overlap-only starts, 128 updates per seed, and
  coefficients 0/.05 before implementation or execution.
- 2026-09-11: Incorporated the user's SIL-weighting suggestion before execution:
  positive current-value advantage, actual returns through real episode termination,
  uniform sequence sampling and the existing PPO critic objective.
- 2026-09-11: Verified full-episode returns across multiple improvements, pending
  segments across chunks, duplicate preservation, routing-only successes, label
  filtering, zero/empty parity, gradient ownership, KL freeze and nonempty replay.
- 2026-09-11: Passed stochastic and scripted mechanical readiness plus actual
  resume from a nonempty buffer/Adam checkpoint. Workspace check/tests passed;
  Python suite: 389 tests run, 7 skipped. Main runs use no scripted actions.
