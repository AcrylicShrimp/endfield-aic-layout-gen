# Blueprint edit contract experiment

Status: complete for the first contract verification slice. Persistent-draft PPO
training is a follow-up milestone, not a result of this experiment.

## Context

The user wants continuing inference to rearrange obstructing facilities across
multiple actions while restoring exactly the original blueprint. Temporary
overlap and disconnected physical routes are allowed only in the private draft.
See [the contract](../designs/2026-09-11-blueprint-edit-contract.md).

## Completion criteria

- [x] New native session with immutable topology, draft edits and validated best.
- [x] Blueprint-aware graph input and experimental actor forward check.
- [x] CLI probe exercising overlap, recovery, topology rejection and long drafts.
- [x] Native regression and workspace verification; concise report and provenance.
- [x] Focused commit and clean worktree.

## Activity

- 2026-09-11: Registered the new contract before implementation. Existing PPO
  learning-rate results are complete; this work changes the editing environment.
- 2026-09-11: Passed five scripted scenarios (57 attempts), nine native guard/
  behavior tests and three new Python tests. Recovered all 15 original directed
  connections after overlap; separate validated compaction reduced area 700 to
  595. Confirmed the audit rejects a rewiring that passes ordinary material and
  physical acceptance. Graph forward/backward consumes blueprint and draft.
- 2026-09-11: Workspace check/test passed (561 passed, five existing ignored);
  Python discovery passed 376 tests (369 passed, seven platform/optional skips).
  Retained selected ignored evidence and the inspected figure; recorded
  [findings](../reports/2026-09-11-blueprint-edit-contract.md) and compact hashes.

## Next milestone

Introduce persistent-draft PPO collection with value bootstrapping across
nonterminal rollout chunks. Train on already improved and partially rearranged
states, then evaluate exact recovery and validated quality against inference
time. Design any repair shaping separately; this slice grants no reward to
overlapping draft area.
