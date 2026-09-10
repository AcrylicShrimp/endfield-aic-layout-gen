# Full-factory environment ownership and performance

## Status

Active. The user replaced the next-work plan with five ordered stages: profile
current execution, compare with the earlier Rust-owned environment, redesign the
Rust contract, implement it, then profile again. The selected actor input is
`current-history-16`. No new learning-quality or long-training run is requested.

## Context

The earlier directed-path pilot completed Rust ownership of state, candidates,
transitions and observations. The later full-factory learning path instead uses
Rust transaction acceptance, a Node action executor and Python observations,
including complete witness JSON transfers during selected actions. The completed
PPO harness provides durable execution and monitoring, but did not complete the
full-factory ownership cutover. Preserve its completed experimental evidence.

## Ordered completion criteria

1. Profile the unchanged current full-factory path in release mode. Separate
   fresh generation, candidate extraction, observation assembly, inference,
   selected edit/routing, native acceptance, serialization/IPC and persistence.
   Retain representative actual inputs/actions/checkpoints, raw profiles and
   source identities under ignored artifacts. Separate instrumentation from
   uninstrumented elapsed-time measurements.
2. Compare measured stages with the retained Rust-owned pilot profiles. Clearly
   distinguish different physical scopes, model sizes, action domains and work;
   do not present cross-workload timings as a controlled language speed ratio.
3. Write the new Rust environment contract using the profile evidence: inputs,
   outputs, state ownership, invariants, failures, diagnostics, evidence exports and
   semantic/performance verification. Preserve current full-factory semantics.
4. Implement the contract and cut the current training/evaluation path over
   together. Remove obsolete normal Node/Python environment paths. Keep any
   required historical oracle isolated for verification. Preserve action domain
   order, physical acceptance, temporary valid worsening, retained-best scoring,
   observations, sampling, actual behavior probabilities and PPO/GAE semantics.
5. Repeat profiling on the same retained inputs/actions and hardware. Verify
   environment/observation parity and actual native collection, interruption,
   continuation and harness integration. Report measured gains and remaining
   costs, retain evidence, pass repository checks and commit completed slices.

## Activity log

- Recorded the user-requested order after identifying that the full-factory
  extension did not retain the earlier native observation/action-index boundary.
- Begin with the existing frozen shallow screen and the historical native-owned
  reports. Preserve unrelated cancelled-study drafts and the retained runtimes.

- Stages 1 and 2 complete: five representative serial release profiles with
  exact retained actor/critic experience parity and one actual 24-worker batch
  with 1,024 matching selected actions. The food setup performs 5,723 repeated
  physical-key computations; Python observations and Node descriptions are
  other substantial costs. Historical native summary hashes match retained
  provenance. See the [baseline report](../reports/2026-09-10.07-full-factory-ownership-profile.md).
- Retained the full baseline runtime, profiles, selected inputs/experiences and
  historical summaries with matching server/workstation archive hashes. Proceed
  to evidence-based full-factory Rust contract redesign.
- Stage 3 complete: [native contract](../designs/2026-09-10.08-full-factory-native-environment.md).
  Stage 4 has a working retained native environment with matching geometry,
  candidate order and observation arrays. The normal PPO/generation callers
  still await cutover; the second profile has not started.
- The user requested a narrower implementation after the work took too long.
  Removed the newly added mid-episode snapshot/restore contract and its tests:
  the existing harness resumes from published episodes and learner checkpoints.
  Keep that boundary and focus remaining work on caller cutover and profiling.
