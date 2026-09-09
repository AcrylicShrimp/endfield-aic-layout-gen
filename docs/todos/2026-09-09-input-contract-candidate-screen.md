# Input-contract candidate screen

## Status

Stopped at the user's request while reconsidering the fixed-reset training
distribution. Fifteen of 24 jobs completed; four were interrupted and five were
not started. Implementation, semantic tests and all-candidate native smoke
verification pass. The registered full comparison and its final audit are incomplete.

## Context

The user requested many input candidates after reviewing the current observation
and restore-best contract, and chose a broad first comparison over longer training
for each candidate. The [contract](../designs/2026-09-09.38-input-contract-candidate-screen.md)
freezes twelve candidates and two initialization seeds. The cancelled optimizer
stability study remains stopped; the separate information-diagnosis draft is not
executed or included as a completed work unit.

## Completion criteria

- All 24 registered candidate/seed jobs finish or retain explicit failure evidence.
- Every complete checkpoint and trace artifact passes identity/accounting checks.
- Three declared episodes per job pass independent native, observation, mask and
  sampling replay.
- Report all registered checkpoint evaluations and both seeds, including regressions.
- Distinguish training cases from previously opened controls held out of this training.
- Retain checkpoints, action traces, witnesses and frozen local source guards under
  ignored artifacts; commit concise findings and reproduction sources only.
- Complete the repository checks and commit the finished unit without staging
  unrelated cancelled-study drafts.

## Activity log

- Defined 12 observation/action-contract candidates and fixed 32 updates per seed,
  four 32-action episodes per update, with evaluation at 0/16/32 updates.
- Implemented shared current/best encoding, explicit input omission, bounded causal
  action history, matched no-restore pairs and episode-gradient accumulation.
- Ten semantic tests pass, including equality with the direct group objective
  and nonempty history gradients through the recurrent encoder.
- All 12 native smoke jobs pass: 576 selected actions, 25.553 seconds coordinator
  wall time and 2.51 GiB sampled peak aggregate RSS. These are execution checks,
  not learning results.
- All smoke checkpoint payloads and episode artifacts pass verification; 144
  selected actions pass independent native/observation/probability replay.
- Workspace check and tests pass (538 tests, two existing ignores); artifact
  worktree check passes.
- Froze and launched the full 24-job screen under
  `artifacts/factory-edit-pilot/full-factory-input-contract-screen/`, with four
  isolated workers, one inference thread each. Its 98,304 training and 36,864
  evaluation selections are planned counts until execution is complete.
- Stopped the coordinator and its owned workers with SIGTERM after 3540.609
  seconds. Retained completed jobs, partial groups, checkpoints and traces.
  No full-screen summary or ranking is published. The proposed twelve-hour
  continuation was cancelled before implementation or execution.
- The follow-up question requires freshly generated production targets, rates
  and initial layouts for every training episode, with parallel collection on
  the existing experimental server. Do not resume this fixed-reset comparison
  or the cancelled long continuation automatically.

- The separately frozen fresh-generation PPO replacement completed all 24 jobs.
  See the [new comparison report](../reports/2026-09-10.06-shallow-ppo-input-comparison.md).
  This does not change the cancelled status or incomplete audit of this older run.
