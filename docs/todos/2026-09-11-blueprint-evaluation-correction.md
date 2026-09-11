# Correct stopped blueprint duration evaluation

Status: in progress.

Context: the user requested stopping training, restoring four-batch evaluation,
and reusing existing metric names. The user confirmed the original 32 factories, fresh training initialization and
a real iteration-zero evaluation datapoint.

Completion criteria:
- Confirm paused training, saved checkpoint, completed tail audit and stopped W&B.
- Restore four-batch evaluation and the confirmed cohort before starting a fresh run.
- Automatically attach configured W&B and reuse established metric axes/keys.
- Verify canonical metrics on existing W&B charts, tests and native readiness.
- Preserve historical evidence, commit, launch a fresh corrected twelve-hour run
  and verify iteration-zero and four-batch records online.

Activity log:
- 2026-09-11: Requested SIGTERM for the actual trainer. It paused after update
  1778 / 455,168 actions in 2,643.185 seconds. Observer finished and acknowledged
  history through step 1780. Tail audit is still running.
- 2026-09-11: Asked which cohort the latest instruction intends; independent
  schedule and canonical metric correction continues.
- 2026-09-11: Cohort clarified as the original 32 factories; fresh model,
  optimizers and replay buffer required, with iteration-zero evaluation first.
- 2026-09-11: Original job tail audit passed; both launchd registrations removed.
- 2026-09-11: Corrected native port-turn heading validation; all 32 unchanged
  historical factories now admit. A development readiness run records 0/4/6
  evaluations and exact tail replay after a real checkpoint pause/resume.

- 2026-09-11: Final online readiness and all six update replays passed; server
  history confirms 32 cases at 0/4/6 and canonical axes. Implementation complete;
  freeze source and launch the fresh twelve-hour run next.
