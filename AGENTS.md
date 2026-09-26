# AGENTS.md

## Scope And Language

- This project generates Arknights: Endfield Automated Industry Complex layouts. Work in small, verifiable increments.
- Write repository content in English. Use Korean when conversing with the user.
- Keep this file limited to durable repository-wide working rules. Put algorithms, experiment settings, temporary decisions, and research procedures in the relevant documents under `docs/`.
- Consult the relevant accepted designs, reports, and active task records before changing a component. Removing detail from this file does not cancel established product or data contracts.

## Reuse Existing Work And Preserve Its Benefits

- Choose the smallest complete design that fits the current need. Do not create a
  new framework, abstraction layer, storage protocol, or generalized extension
  point when an established module or a direct addition already solves the task.
- Before building a new component, harness, or replacement, inspect the existing implementation and its relevant designs, configuration, tests, and performance findings. Identify what can be reused and which solved problems must stay solved.
- Reuse compatible code and infrastructure as much as practical. If a different implementation is needed, carry forward the existing benefits: correctness, features, performance optimizations, resource controls, observability, and recovery behavior. A new architecture is not permission to restart from a slower or less capable baseline.
- Carry forward applicable optimization techniques even when direct code reuse is
  impossible. Avoid predictable design-level regressions such as repeated setup,
  per-item initialization, unnecessary copies, unbounded queues, redundant
  serialization, or rescanning stable data; these should be prevented before the
  first integrated run rather than discovered through a long benchmark campaign.
- In particular, preserve applicable batching, caching, persistent workers, compact data representations, and efficient serialization, storage, and validation. Do not replace an optimized path with naive per-item work, repeated initialization, full-state copies, or repeated disk I/O without a demonstrated need.
- For substantial replacements, record a short continuity checklist in the relevant design or task record: existing benefit, reuse or replacement approach, and how it will be checked. Investigate uncertain applicability instead of silently dropping an item.
- Verify the actual integrated workflow before adopting the replacement or launching expensive downstream work. Reuse retained evidence and make a bounded, comparable runtime measurement when performance is affected, including storage and validation costs. A successful build or correct final example alone does not establish continuity.
- Restore omitted established requirements within the work. Intentional removal, weakening, or deferral of a user-required capability needs explicit user authorization; do not relabel it as an optional follow-up.

## Keep Normal Execution Efficient

- Keep runtime validation at stable input and publication boundaries, cache or
  reuse successful results while identity is unchanged, and keep checks out of
  hot loops. A normal run should emit compact summaries and required recovery
  state, not exhaustive traces, repeated snapshots, or large diagnostic bundles.
- Prove internal module output invariants through module design and offline tests
  or explicit bounded audits. Do not repeatedly revalidate trusted internal
  outputs per sample, batch, or step in normal execution. Keep necessary checks
  for external input admission, publication, and actual resource exhaustion.
- Keep training and other normal workloads focused on useful work. Give added persistence, validation, and diagnostics a concrete purpose and a frequency proportional to that purpose.
- Simplify state saving, recovery, and validation by default. Save only the state needed for the required resume behavior, reuse existing checkpoint mechanisms, and prefer a direct restore path. Avoid duplicate snapshots, redundant journals, and extra recovery layers.
- Do not impose exact historical replay, exhaustive provenance, or full revalidation on routine recovery unless an explicit contract requires it. Choose checkpoint frequency for the required recovery needs and measured I/O cost, not maximum possible durability.
- Validate stable inputs at the appropriate boundary and reuse the result while their identity remains valid. Avoid repeated hashing, replay, directory scans, full snapshots, and duplicate computation in hot loops.
- Preserve required input rejection, correctness, committed-output validity, resource limits, and recovery guarantees. Improve how and where checks and saves run instead of weakening these contracts.
- Keep expensive audits explicit, bounded, and separate from normal runs. Do not add them per step, per batch, periodically, or at every completion by default. End diagnostic processes and restore normal settings before a long run.
- Use existing evidence and the smallest sufficient check. Do not rerun successful checks or workloads without a new change, failure, or specific unresolved question.

## Training Server

- Run GPU training and CUDA performance measurements on `ashrimp-arch`, accessed
  with `ssh ashrimp@ashrimp-arch`. Keep every server checkout, build, temporary
  input and retained result under `~/Srv`; see
  `docs/maintenance/training-server.md` for the current runtime path and checks.
- Use local hardware only for bounded compilation, unit tests and smoke runs when
  the server is available. Check server disk capacity and active GPU processes
  before building or launching training. Remove only regenerable caches without
  explicit evidence-retention review.
- Attach every long-running training experiment to the configured W&B project
  before leaving it unattended. Keep local committed state authoritative, use a
  nonblocking resumable observer so tracking cannot stall training, and verify
  that W&B acknowledged a real readiness record before the long launch.

## Implementation And Contracts

- Keep architecture simple. Prefer complete internal cutovers, remove obsolete paths, and avoid unnecessary adapters or compatibility shims. Report what became simpler after a cutover.
- Define inputs, outputs, invariants, failure modes, and useful diagnostics before implementing a substantial stage. Make external data or CLI breaking changes explicit through schema versions, migration notes, or diagnostics.
- User-facing interaction is through CLI commands. Prefer explicit subcommands and flags; errors should identify the failing input or path. Structured diagnostics should include a stable code, severity, stage, message, and relevant entity.
- Load game data from external files on each execution; do not hardcode facilities, recipes, items, transport, or balance values in application code.
- Preserve the relevant training input, evaluation separation, episode identity, and resume contracts when replacing learners, environments, or harnesses.
- Keep production behavior and research tooling separate. Do not silently fall back to a research solver, weaken a problem to obtain success, or report a timeout, exhausted budget, or local failure as global infeasibility. Example bounds are not system limits.

## Design And Implementation Review Gates

- Before accepting a design or implementation plan, obtain an independent fresh-context review against the user's requested outcome, existing accepted contracts, and current capabilities. The reviewer must trace every required responsibility to a concrete design and verification path, including difficult cases and integration boundaries. A design that says to implement a simpler subset now and complete a required capability later does not pass merely because the subset is easier to test.
- Make each approved stage complete for its stated purpose. Do not silently turn an end-to-end requirement into a research-only boundary, narrow the supported problem or input domain, or defer component legalization, recovery, performance, or other required behavior. Record any genuine scope change and obtain explicit user authorization before treating it as accepted; a TODO, limitation note, or future-work section is not authorization.
- After implementation, obtain a separate independent fresh-context review before calling the stage complete, promoting its result, or launching expensive downstream training. Give the reviewer the accepted design and requirements, the implementation diff and relevant code paths, and the integrated verification evidence. The reviewer must check missing and weakened requirements as well as bugs, incorrect behavior, and regressions in previously supported capabilities.
- Review each requirement from input through output: how it is represented, generated or processed, validated, and exposed to the user. Exercise at least one case that genuinely requires each critical capability; a passing example that can avoid the capability does not prove it is implemented. Report which requirements were demonstrated, which remain unverified, and which are absent.
- Treat a verified missing required capability or an unapproved scope reduction as a blocking review finding. Fix it and repeat the review before reporting completion. If the work remains incomplete, label it as such and do not present bounded prototype metrics as proof of full-system readiness.

## Rust Conventions

- Use Rust stable, a virtual Cargo workspace root, and resolver `3`. Place crates under `crates/` with names prefixed by `aic-`; share dependency versions at the workspace root when practical.
- Use `foo.rs` with child modules under `foo/`; do not add `mod.rs` files.
- Before adding a dependency, run `cargo search <crate> --limit 1`. Use the broadest safe requirement: major-only for versions above zero, minor for `0.x`; document any full patch pin.

## Files And Artifacts

- Keep designs and reports under `docs/`, and near-term task trackers under `docs/todos/`. Task trackers should include status, context, completion criteria, and an activity log.
- Keep regenerable outputs out of Git. Use ignored `target/` for disposable builds and experiments, and ignored `artifacts/` for retained local checkpoints and evidence. Neither is a backup.
- Commit concise findings, reproduction commands/configuration, scripts, and compact provenance manifests. Keep required runtime data and justified minimal fixtures tracked; do not blanket-ignore file extensions or force-add raw reports.
- Register benchmark source assets in `docs/maintenance/benchmark-source-assets.json` and the matching `docs/benchmarks/.gitignore` exceptions.
- Keep each `docs/` file at most 100 KiB, other source files at most 512 KiB, and unmodified upstream game-source tables at most 4 MiB. Reduce or externalize oversized content; do not evade limits by compression, minification, or arbitrary splitting.
- Clean up regenerable intermediate outputs after retaining the summary and necessary results and inputs. Preserve non-replayable checkpoints and irreplaceable evidence; a seed alone may not reproduce an interrupted run.
- Close out every completed, stopped, or superseded experiment before starting
  another substantial run. The closeout package must retain the experiment's
  question and method, exact code identity plus any uncommitted patch, reviewed
  configuration, reproduction command, compact result metrics, verdict,
  interpretation, and provenance needed to reconstruct the test. Commit the
  durable report and source configurations under `docs/`; a server artifact is
  not the only copy of a conclusion.
- After verifying that the closeout package is readable and the code and
  configuration are reconstructible, remove the experiment's replay buffers,
  generated training inputs, intermediate checkpoints, raw profiles, temporary
  panels, build caches, and other executable state. Retain a checkpoint only
  while an explicit continuation or recovery requirement remains active; package
  and remove it when that requirement ends. Record the removed paths and disk
  space before and after cleanup in a compact cleanup manifest.

## Verification And Git

- Keep verification minimal and tied to a credible critical failure. Add or run
  only the smallest check needed to protect correctness, prevent costly data
  corruption, or validate the user-visible workflow changed by the work.
- Do not build exhaustive test matrices, duplicate established coverage, or test
  every validation and resource-limit branch by default. Prefer one bounded real
  workflow check when it exercises the same contract more directly.
- Reversible, noncritical defects may be corrected when they appear in actual use.
  Preserve useful failure evidence, but do not delay delivery to anticipate every
  such case.
- Keep verification proportional to the concrete failure mode and change impact. For CLI-visible changes, run the relevant command and inspect its output.
- After Rust edits, run `cargo fmt --all`. Before completing code changes, run `cargo check --workspace`; run `cargo test --workspace` when behavior changes or tests exist.
- After artifact or documentation changes, run `node docs/maintenance/check-tracked-files.mjs --worktree`. After staging and before committing, run `node docs/maintenance/check-tracked-files.mjs --index`.
- Preserve unrelated user changes. Inspect `git status --short` before committing and stage only the completed work unit.
- Commit completed, verified work unless the user asks otherwise. Keep commits focused; do not commit incomplete work without an explicit checkpoint request.
- Report the commit hash and whether the worktree is clean.
