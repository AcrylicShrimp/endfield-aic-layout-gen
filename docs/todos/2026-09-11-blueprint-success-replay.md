# Accumulating policy-discovered success replay

Status: complete.

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
- 2026-09-11: Completed both registered 65,536-action arms. Reconstructed all
  512 learner/buffer updates and every main training action choice; native
  witness and frozen evaluation prediction audits pass.
- 2026-09-11: Treatment retains 22 unique sequences from 25 successes and has
  71,370 positive-weight label draws. Final mean area reduction rises from .83%
  to 1.67%, but both policies remain below their initial 7.92% and have no final
  area gain through 256 actions. Retention works; robust efficacy is not established.
