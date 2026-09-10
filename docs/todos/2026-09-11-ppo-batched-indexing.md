# PPO batched spatial indexing

Status: complete.

Context: data-supply optimization exposed model dispatch and spatial indexing
backward as remaining costs. The user approved batching those operations.

Completion criteria:

- [x] Define input, output, numerical and measurement contracts.
- [x] Implement grouped spatial indexing with independent factory semantics.
- [x] Compare CUDA forward/gradients, continuation, timing and memory.
- [x] Verify native readiness and repository regressions.
- [x] Record findings, remaining costs and commit the verified change.

Activity log:

- 2026-09-11: Frozen `d22474b` as the existing batched learner baseline. Keep
  the stopped long-run checkpoint untouched and use bounded diagnostics only.
- 2026-09-11: Implemented the user's requested maximum-extent padding and explicit
  validity mask, including normalization, spatial layers and pooling. Removed
  per-world GPU spatial references; learner execution schema is v3.
- 2026-09-11: Preserved failed strict continuation evidence and measured full
  parameter/probability differences. New executions reproduce themselves exactly;
  historical and new learning are not bit-exact. Final evaluation changed seven
  best geometries out of 32 despite identical training result summaries.
- 2026-09-11: Resident minibatch time improved by 30.8%, repeated full updates by
  15.0%, and bounded native readiness by 12.5%. All 79 server PPO tests, native
  validation/continuation audit and workspace checks/tests passed. See the
  [report](../reports/2026-09-11-ppo-batched-indexing.md) for memory and remaining costs.
