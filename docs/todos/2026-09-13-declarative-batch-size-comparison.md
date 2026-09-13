# Declarative batch-size comparison

Status: complete.

Context: user authorized 256 versus 1,024 rollout transitions with equal training
actions, fixed minibatch 64 and retained 80/10/10 starts. See matching design.

Completion criteria:
- Close logical-worker/input/curriculum continuity with bounded integrated checks.
- Run both registered arms for both seeds without changing other learning settings.
- Report paired evaluation, realized curriculum, actual input coverage and runtime.
- Preserve checkpoints/inputs, check artifact policy and commit completed work.

Activity:
- Inspected the completed learner, input binding, curriculum, transport, GAE,
  checkpoint, evaluation, report and previous performance evidence.
- Registered equal-action protocol and explicit consequences of different wave sizes.
- Closed bounded readiness in a separate 152.50-second process: seven tests,
  eight-stream behavior equivalence, six actual updates per batch, unfinished
  episode process restoration, terminal completion and actual fresh-input pairing.
- Existing eight learner tests passed with the selected release native library.
- Prepared four arms from the matching source/native/protocol receipt; previous
  trained checkpoints remain untouched. Matrix and reporting remain pending.
- Completed all four arms in 76.52 minutes: 131,072 training actions, 2,048
  complete episodes, 768 validated evaluation episodes and matching paired
  actual-input identities. Integrated report command passed without new rollouts.
- Recorded the common 13-small/14-large disjoint cohort, seed-dependent quality,
  runtime, RSS, realized curriculum and 1,002 actual generated-input coverage.
- Preserved final/selected/recovery checkpoints, replay source chunks and input
  banks outside Git; tracked the concise report and 5 KiB provenance manifest.
- Workspace check/tests and all fifteen targeted Python tests passed; final
  artifact/index verification accompanies the completed commit.
