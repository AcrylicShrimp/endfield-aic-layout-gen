# Policy-created draft recovery diagnosis

Status: complete.

## Context

The user requested probing actual post-pose drafts and their policy continuation.
See the [registered contract](../designs/2026-09-11-blueprint-post-pose-recovery.md).

## Completion criteria

- [x] Census source poses and restore authenticated post-action native states.
- [x] Separate geometric blockers from bounded fixed-placement routing results.
- [x] Compare counterfactual recovery and score with actual policy continuation.
- [x] Audit successful witnesses and retain source-bound diagnostic evidence.
- [x] Publish concise findings/figures, run checks and commit.

## Activity

- 2026-09-11: Registered the population, geometry classification and bounded
  routing cascade before executing post-pose recovery probes.
- 2026-09-11: Readiness and two native-backed state/fork tests passed. Completed
  the census of 2,597 source poses and 2,467 accepted post-pose states in 66.37
  seconds. Fixed-pose recovery validated 874 states, including 32 rescued by
  reversed pending-edge order. Increased work/window bounds alone rescued none.
- 2026-09-11: Audited all source states and 986 counterfactual/actual witness
  references; 461 unique witnesses passed independent physical validation.
  Recorded 46 area-improving opportunities in 25 episodes: four direct commits,
  16 restore-best boundaries and 26 further-pose boundaries.
- 2026-09-11: Published the [findings](../reports/2026-09-11-blueprint-post-pose-recovery.md),
  selected actual-draft figures and compact provenance. Workspace tests and
  the Python suite passed; generated evidence remains ignored under artifacts.
