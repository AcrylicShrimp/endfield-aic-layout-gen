# Compact action cutover and fresh trial

## Status

Implementation verified locally. GPU readiness and the fresh timed session remain.
See the [action contract](../designs/2026-09-10.13-compact-factory-actions.md).

## Context and completion criteria

The user stopped the earlier trial after discovering one-strip contraction and
requested an audit of unnecessarily fragmented actions, a compact correction,
and another run. Keep 32 decisions, the selected actor, eight workers and existing
PPO settings. Preserve the earlier evidence; initialize fresh. Complete native
invariant tests, normal workspace/PPO checks, a bounded CUDA readiness run, and a
new isolated one-hour session. No automatic twelve-hour continuation.

## Activity

- Audited all eight action families. Change contraction, insertion and component
  destination enumeration; retain the already atomic pose/port/route/release/restore
  operations. Define fixed-size descriptors and explicit candidate caps first.
- Implement axis-wide contraction, bounded multi-cell insertion and direct
  translated/rotated component proposals. Reuse two spare action features; keep
  all tensor widths and the model unchanged. Replace obsolete whole-vocabulary
  and observation parity tests with four native invariant/acceptance tests.
- Local checks pass: `cargo fmt --all`, `cargo check --workspace`,
  `cargo test --workspace` (545 passed, four explicit historical tests ignored),
  and all 44 `test_ppo*.py` tests using the newly built native extension.
  The crossing fixture compresses both axes in two charged decisions. Multi-cell
  insertion validates and its contraction inverse preserves physical identity;
  a distant component translation plus rotation also validates as one edit.
- Schema cutover: input `factory-input-contract-screen-v2`, PPO experiences
  `ppo-critic-gae-direct-area-v2`, harness/checkpoints `ppo-training-harness-native-v3`.
  The old frozen runtime remains the reproduction path for old checkpoints.
