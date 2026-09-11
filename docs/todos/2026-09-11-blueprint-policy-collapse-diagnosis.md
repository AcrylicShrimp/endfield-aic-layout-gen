# Diagnose early blueprint policy deterioration

Status: complete.

Context: the user observed zero area reduction and requested further analysis.
Training updates continue, but facility-pose selections dropped sharply.

Completion criteria:
- Authenticate a fixed evidence cutoff without interrupting training.
- Compare identical-state policy probabilities and per-action reward/credit.
- Verify exact replay before interpreting isolated update counterfactuals.
- Report confirmed mechanisms, uncertainty and the next bounded experiment.
- Commit the diagnostic source, concise findings and provenance.

Activity log:
- 2026-09-11: Registered the offline diagnostic contract; active trainer unchanged.

- 2026-09-11: Verified 40,960 rewards through update 160, including 2,063
  rejected actions; none are negative. The older failure-cost module is unused.
- 2026-09-11: Fixed-state probes and seven exact replay controls with isolated
  loss ablations identify influential zero-reward PPO credit scaling.
- 2026-09-11: Corrected diagnostic Adam aliasing before interpreting ablations;
  isolation/parity regressions and all repository verification passed.
- 2026-09-11: Retained report, figure and provenance; live trainer unchanged.
