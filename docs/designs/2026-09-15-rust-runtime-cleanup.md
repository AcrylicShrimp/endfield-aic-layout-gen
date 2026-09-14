# Rust runtime cutover after collection verification

Status: implemented. The user requested leaving Python source intact while
removing obsolete Rust dependencies, compatibility paths and porting-only checks,
loads and persistence. This supersedes the extension-preservation and diagnostic
CLI portions of the numerical foundation and native collection designs.

## Ownership and retained contracts

The Rust workspace no longer builds a Python extension. `aic-pilot-native` is an
rlib containing the declarative editor, its actual topology/geometry/routing,
retained candidate inputs and worker pool. The old single-path engine, transactional
factory engine, manual blueprint editor, Python bindings, optional Python feature,
cdylib, extension build fingerprint and their unused helpers are deleted. Python
source under `tools/` remains untouched as historical code; executing its old
extension-dependent harness requires the pre-cutover checkout and environment.
There is no forwarding shim, alternate feature or old-command alias.

`aic-learning` has one actual minibatch update entry point accepting a frozen
`PpoBatch` and `ReplayBatch`. It no longer accepts a porting manifest as an update
argument. The old kernel runner, standalone numerical checkpoint format,
checkpoint comparator and diagnostic RNG draws are removed. Model/Adam/RNG math,
ordered conditional choices, actor-safe critic updates, numerical precision and
native work limits retain their established semantics.

`learning collect` performs the existing bounded collection/minibatch stage. It
accepts a positive chunk count within the configured episode horizon; the old
two-chunk diagnostic cap is gone. Full epoch/KL scheduling, success-buffer
admission/retention, input generation, curriculum/evaluation and complete-wave
publication remain required for the later training harness. This cleanup does
not present the current command as that complete harness.

## Work removed from execution

| Removed work | Direct replacement or retained guarantee |
| --- | --- |
| Kernel/collection oracle outputs, before/after snapshots, first-wave RNG export, gradient archives, copied numerical inputs and credit/transition dumps | No runtime replacement. Retained fixtures are exercised only by explicit ignored Rust regression tests. |
| Loading, validating and transferring old PPO rows which are immediately overwritten | Actual rollout constructs `PpoBatch` directly. Seed contains only initial learner state and historical replay. |
| Keeping all imported model/Adam tensors for the run | Consume initial state once and release it after learner construction. |
| Reading original model/Adam state during restore | Separate state/replay seed archives; resume reads replay and the boundary checkpoint. |
| Per-update model/config equality and duplicate external tensor shape checks | Bind configuration at construction; validate external replay once at admission. |
| Per-stream hashing of identical catalogs | Validate/load catalogs and calculate their identity once for the pool. |
| Initial numerical observation built at admission and rebuilt at chunk start | Admission returns readiness; construct the first observation at chunk start. |
| Full JSON observations, diagnostic root/metadata exports and manual close mode | Typed retained observations and episode/resource horizons. |
| Per-chunk full evidence identity re-audit | Native ownership and immutable historical inputs remain structural invariants, covered by regression tests. |
| Fresh validation of an already validated retained best at completion | Return the native retained result. External cold audit is explicit. |
| Revalidating equal initial/current/best states during restore | Reuse the admission/state validation result when states are equal; validate each distinct required state. |
| JSON writes issuing a syscall for each tiny token | Buffered writes with explicit error-reporting flush; atomic checkpoint publication and fsync remain. |

Required checks stay at their boundary: external schemas/hashes/shapes/ownership,
held-out reservation, native handle ownership, action/time/routing limits,
nonfinite model/loss rejection, retained-input memory budget, and checkpoint
binding/state validation. Successful-step preparation evidence and positive
witnesses remain available for replay integration. Failure retains partial actual
inputs, choices, native evidence and learner state; it never publishes a completed
boundary. There is one boundary checkpoint, not a second numerical recovery path.

## External changes

- Removed CLI commands: `learning verify-kernel`, `learning verify-collection`,
  `learning compare-checkpoints`.
- Current command: `learning collect --config CONFIG --seed SEED --output NEW`,
  with `--chunks`, `--device`, `--workers` and `--resume`.
- Seed schema: `aic-learner-seed-v1`, replacing diagnostic kernel interchange.
  `state` and `replay` each contain a sibling `path` and `sha256`. Model/update
  configuration, parameter order, SIL graph/factor counts and the tensor byte
  budget remain explicit. PPO fields are absent.
- Boundary schema: `aic-native-collection-checkpoint-v2`, bound to `seed_sha256`;
  embedded native session schema is `declarative-edit-checkpoint-v2` without the
  old manual-close field. Report schema is `aic-native-collection-report-v2`.
- Old diagnostic checkpoints are retained as evidence. No runtime migration or
  fallback to v1 is introduced. Use the pre-cutover checkout for historical runs.

See the cleanup report for one-time seed extraction, measured costs and verification.
