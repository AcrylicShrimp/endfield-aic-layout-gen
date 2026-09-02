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

## Constructive Planner And Local Solver Policy

- The production architecture must construct a complete valid factory first and improve that concrete layout iteratively afterward.
- Construction grows a connected production subgraph incrementally. Add one frontier facility or atomic cycle, assign the newly required ports, and route the new connection before expanding the frontier again. Do not place the entire facility set and defer all routing to a later global phase.
- Treat each frontier expansion as a transaction. When it cannot place or route the new input, retry alternative local placement, rotation, port, and route choices; if necessary, roll back a bounded recent growth window without discarding unrelated validated regions.
- Facility placement, port assignment, logical commodity-network synthesis, and physical routing may use deterministic constructive heuristics. These heuristics are now the approved production path; they do not claim global optimality or global infeasibility.
- Use a dedicated routing engine rather than a global placement-routing constraint model. Route pipes first, use A* or Dijkstra with length, turn, and congestion costs, and support rip-up-and-reroute.
- Synthesize one logical network per compatible commodity and transport kind before embedding it into the grid. Shared trunks and trees are preferred; splitters and convergers are derived from the embedded result.
- CP or SAT solving is a local oracle. Approved uses include port assignment for a bounded facility group, local facility relocation, local packing, and proof that a precisely defined local neighborhood is impossible.
- A local proof must not be reported as proof that the whole factory request is infeasible.
- Improvement passes operate transactionally. Keep the current valid layout unless a candidate validates and improves the configured score.
- Preserve the score priority: used bounding-box area first, physical belt and pipe tile count second, total route turns third, followed by later tie-breakers.
- The initial constructor may be visually poor. Every committed growth state must be valid for its covered production subgraph, and its final acceptance criterion is a complete validated factory at whatever natural footprint construction produces.
- A construction failure is a structured planner failure, not a global infeasibility proof. Diagnostics must identify the failing stage, network, facility group, or exhausted bound.
- Keep the former global exact joint solver as isolated research tooling. Production APIs and normal CLI commands must not invoke it, automatically fall back to it, or expose its timeouts as production planner failures.
- Before committing each planner slice, compare it against `docs/designs/2026-09-02.00-constructive-layout-planner.md` and this policy.

## Exact Search Research Policy

- The global joint placement-routing model is retained only as an explicit research instrument and historical exact baseline.
- Keep its APIs, commands, reports, and dependencies separated from the production constructive planner. New production behavior must not depend on research model construction or search completion.
- Search-space explosion in the faithful joint model is an expected research result. It is evidence to measure, not a production failure.
- Establish and measure the faithful exact baseline before attempting to reduce its search space. It is acceptable for the baseline to time out or return `unknown`, including in an early SCC phase.
- Do not replace, bypass, crop, or pre-decide the difficult part of the model merely to obtain a feasible demonstration. That removes the research subject instead of improving the solver.
- Push every applicable game rule into the solver formulation so that propagation can eliminate invalid placement, port, and routing combinations as early as possible.
- Search-space reductions must preserve every legal solution and the configured objective quality. Each reduction must be justified as a sound constraint, exact reformulation, dominance rule, symmetry breaker, or completeness-preserving algorithmic transformation.
- Measure model construction and search separately. Record variable and constraint counts, construction time, search time, incumbent objective, best bound when available, termination reason, and the phase at which growth becomes intractable.
- A timeout or exhausted resource budget must return structured evidence and any complete solver incumbent already found. It must not trigger an unapproved heuristic fallback.
- Research should focus on stronger exact formulations, propagation, redundant valid constraints, symmetry removal, incumbent bounds, and solver-native hints from prior exact solutions.
- Preserve hard resource limits for operational safety, but never present a resource-limited `unknown` result as infeasibility or as justification for silently changing the problem.

## Propagator Research Review Policy

- Use `docs/designs/aic-game-rules-mathematical-model.md` as the shared semantic reference for custom propagator work. Update that model when an accepted rule changes or new evidence resolves an ambiguity.
- Before finalizing each new or materially changed custom propagator, request multiple independent reviews that separately examine proof soundness and bugs, exact optimization opportunities, and alternative or follow-up propagator strategies.
- Reviewers must distinguish confirmed game or solver semantics from implementation behavior and unresolved domain ambiguity. An unresolved ambiguity is not a valid pruning premise.
- Verify every actionable review finding locally before changing code or reporting it as confirmed. Record accepted fixes, rejected or inconclusive claims, verification commands, and remaining risk in the slice report.
- Prefer one semantic proof rule per propagator. Shared immutable indexes and reporting utilities are allowed, but unrelated inference rules should not be hidden behind one runtime mode switch.

## Layout Bound Semantics

- Production constructive planning has no caller-supplied W/H limit. Width and height are measured outputs and optimization objectives, not feasibility constraints.
- Treat legacy `max_width` and `max_height` fields as explicit research-solver canvas ceilings for that one diagnostic request only. They are not production planner inputs, system limits, required blueprint dimensions, target dimensions, canonical game limits, or default model sizes.
- Do not promote any diagnostic or example request bound into a project invariant or MVP success criterion. A test bound may exist only to evaluate that specific request or to determine whether an earlier bound was too restrictive.
- Actual layout width and height come from the used facility, belt, pipe, and logistics-component geometry selected by the planner or research solver. Unused search capacity is not part of the blueprint footprint.
- Exact solving does not require eagerly materializing every cell inside a loose maximum bound. Symbolic, sparse, or otherwise compact exact formulations are allowed when they preserve every legal solution and the configured objective.
- Never hardcode an example request bound into planner or solver architecture, runtime data, benchmark acceptance, or documentation claims about system limits.

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
- After completing a work unit and passing the relevant verification, commit the resulting changes unless the user explicitly asks not to commit.
- When one turn completes multiple separable work units, split commits by work unit when practical.
- Do not commit incomplete work unless the user explicitly requests a checkpoint commit.
- After committing, report the commit hash or hashes and whether the worktree is clean.
