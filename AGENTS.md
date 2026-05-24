# AGENTS.md

## Language Policy

- Write all repository content in English, including code, comments, commit messages, documentation, TODO files, and configuration text.
- Use Korean only when conversing with the user outside repository files.

## Project Scope

- This project targets layout generation for the Arknights: Endfield Automated Industry Complex.
- Do not try to complete the full optimizer in one step. Work through the roadmap incrementally.
- Prefer small, verifiable milestones over broad speculative design.

## Rust Workspace Rules

- The repository root must remain a virtual Cargo workspace root.
- Use Rust stable.
- Use Cargo workspace resolver version `3`.
- Place all crates under `crates/`.
- Prefix every crate name with `aic-`.
- Keep shared dependency versions in the root workspace manifest when practical.

## Rust Module Layout

- Do not use `mod.rs` files for new modules.
- Use the modern paired-file layout: `foo.rs` for the module entry point and `foo/` for its child modules.
- When splitting an existing module, prefer `foo.rs` plus `foo/bar.rs`, `foo/baz.rs`, and similar child files.

## Dependency Policy

- Before adding a dependency, run `cargo search <crate> --limit 1` to confirm the current latest published version.
- In `Cargo.toml`, specify the broadest safe version requirement.
- For crates with a major version greater than zero, specify only the major version, such as `"4"`.
- For crates with major version zero, pin the minor version, such as `"0.3"`.
- Do not pin full patch versions unless there is a documented compatibility reason.

## Data Policy

- Do not hardcode facility, recipe, item, belt, pipe, or game-balance data in application code.
- Runtime data must come from external files loaded on each execution.
- Schema definitions and validators may live in code, but actual game data must remain external.

## CLI Policy

- All user-facing interaction is through command line interfaces.
- Prefer explicit subcommands and flags over implicit behavior.
- Error messages should explain what input or file path caused the failure.

## Architecture And Cutover Policy

- Prefer hard cutovers when replacing an internal contract or architecture.
- Do not preserve obsolete interfaces through unnecessary bridging, rerouting, adapter layers, or compatibility shims.
- During a cutover, it is acceptable for the project to fail to compile temporarily.
- Complete cutovers must remove obsolete code paths and restore normal verification.
- After a cutover, report what became simpler compared to the previous design.
- Keep architecture as simple as possible. Add complexity only when it is required by an explicit contract, diagnostic requirement, or user-facing behavior.
- Prefer the new contract over reusing old code. Delete or rewrite old code when reuse would distort the new design.
- For external data or CLI contracts, make breaking changes explicit through schema versions, migration notes, or diagnostics instead of preserving obsolete internal code paths.

## Contract And Diagnostic Design

- Before implementing a substantial stage, define the stage contract first.
- Stage contracts should specify input DTOs, output DTOs, invariants, failure modes, and diagnostic/report schemas.
- Diagnostics are first-class outputs. Design them so callers can observe quality, failure causes, and important internal decisions without reading implementation details.
- Each diagnostic should have a stable code, severity, stage identifier, human-readable message, and relevant entity reference when available.
- Do not treat correct final output as sufficient evidence that a stage is well designed.
- Prefer explicit, structured reports over ad hoc logging for stage behavior that needs to be inspected or tested.

## Documentation And TODOs

- Keep design notes and temporary task trackers under `docs/`.
- Use `docs/todos/` for near-term tracked work.
- TODO files should include status, context, completion criteria, and an activity log.

## Verification

- Run `cargo fmt --all` after Rust edits.
- Run `cargo check --workspace` before considering code changes complete.
- Run `cargo test --workspace` when behavior changes or tests exist.
- For CLI-visible behavior, run the relevant command manually and verify its output.

## Git

- Keep commits focused and descriptive.
- Do not revert unrelated user changes.
- Before committing, inspect `git status --short`.
