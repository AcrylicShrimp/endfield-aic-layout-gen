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
- Normal learned-edit training must generate fresh production products, rates, and complete randomized initial layouts for each new episode. Preserve this input contract across learner or environment replacements. Fixed factories are explicit diagnostics only, never an implicit main-training input or fallback.
- Bind every training episode to its actual generated request and initial layout. Preserve unfinished episodes across rollout chunks and process restoration without regenerating their inputs. Keep evaluation inputs separate; report generation coverage and failures alongside learning results.

## Generated Artifact Policy

- Do not commit regenerable run outputs by default: raw solver or training traces, per-candidate JSON, generated HTML/SVG views, logs, and build caches belong outside tracked source directories.
- Put disposable build and experiment outputs under ignored `target/`. Put retained local checkpoints or evidence under ignored `artifacts/`, outside Cargo cleanup. Neither location is a backup.
- Commit concise human-written findings, reproduction commands/configuration, source scripts, and compact provenance/hash manifests. Keep required runtime data and deliberately selected test fixtures tracked; do not blanket-ignore JSON, HTML, or SVG extensions.
- Before retaining generated content in Git as a fixture or essential evidence, document why it is needed and keep the smallest sufficient payload. Do not force-add raw output just because it was used in a report.
- Benchmark source assets must be explicitly listed in `docs/maintenance/benchmark-source-assets.json` and the matching `docs/benchmarks/.gitignore` exceptions. Generated reports are not source assets merely because another experiment reads them; use explicit external inputs or extract a minimal independent fixture.
- Keep each file under `docs/` at or below 100 KiB, including human reports, handoffs, patches, manifests and fixtures. Non-regenerability is not an exemption. Summarize or externalize oversized records; do not evade the limit through compression, minification or arbitrary chunking.
- Other source files have a 512 KiB review limit. Unmodified upstream game-source tables have a separate 4 MiB ceiling because they preserve identified external source snapshots; this is not an exemption for research output. Review growth before raising any limit.
- Clean up regenerable intermediate candidates after recording the run summary and preserving required selected results and inputs. Preserve non-replayable checkpoints and irreplaceable evidence; a seed alone does not guarantee an interrupted training stream can be reproduced.

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

## Preserve Established Work Across Changes

- Treat every change as a continuation of this project, not a blank slate. Existing accepted requirements, design decisions, solved problems and verified capabilities remain obligations even when the latest request does not repeat them. A rewrite, new component or hard cutover does not by itself authorize discarding them.
- Before substantial work, inspect the relevant existing implementation, decisions, reports, configuration and tests. Record a concise continuity checklist in the slice's design or TODO: what already works, why it exists, which prior failures it prevents, where it belongs after the change, and how its preservation will be verified. Do not infer the full contract from a small example or the latest prompt alone.
- Consider the whole system: user-visible behavior and workflows, data and semantic contracts, correctness guarantees, edge cases, failure handling and recovery, observability, resource controls, performance, verification, and operational procedures. Preserve established benefits and safeguards, including those outside the component being rewritten. If applicability is unclear, investigate it and keep the item open; do not silently assume it is obsolete.
- Reuse compatible existing work. When the new design needs a different implementation, carry forward the underlying requirement and benefit through that design without unnecessary compatibility shims. Simplifying structure is welcome; deleting a capability, guarantee or accumulated knowledge is not a simplification by itself. Review previously resolved problems and rejected approaches before reintroducing them.
- Verify continuity through the actual integrated command or workflow. The presence of an old module, a copied setting, a similar name, or a passing isolated example does not prove that the new path preserves its behavior. Exercise representative normal, boundary, failure and recovery cases, and compare with retained predecessor evidence. Where a claimed benefit is quantitative, measure it under comparable conditions and state material differences.
- Before declaring substantial replacement work complete, using it as the normal path, or launching expensive downstream work through it, close the continuity checklist. A correct final example or a successful build alone is insufficient. Completion reports must account for established capabilities and decisions, verification evidence, regressions or tradeoffs, and any explicitly authorized exceptions.
- Do not reframe an omitted established requirement, solved problem or working capability as a newly discovered optional enhancement. Restore omissions within the authorized work. Intentional removal, weakening or deferral of a user-required capability needs explicit user authorization; permission to redesign its implementation is not permission to remove it.

## Constructive Planner And Local Solver Policy

- The production architecture must construct a complete valid factory first and improve that concrete layout iteratively afterward.
- Construction grows a connected production subgraph incrementally. Add one frontier facility or atomic cycle, assign the newly required ports, and route the new connection before expanding the frontier again. Do not place the entire facility set and defer all routing to a later global phase.
- In the user-approved downstream-closed growth experiment, a transaction may include the connected downstream dependency group needed to finish a shared producer's internal outputs. Add and route its facilities incrementally in private, charge all work to the existing transaction budgets, and commit only the validated closed group. This is not permission for global facility-only placement or relaxed external-boundary semantics.
- Treat each frontier expansion as a transaction. When it cannot place or route the new input, retry alternative local placement, rotation, port, and route choices; if necessary, roll back a bounded recent growth window without discarding unrelated validated regions.
- Facility placement, port assignment, logical commodity-network synthesis, and physical routing may use deterministic constructive heuristics. These heuristics are now the approved production path; they do not claim global optimality or global infeasibility.
- Use a dedicated routing engine rather than a global placement-routing constraint model. Route pipes first, use A* or Dijkstra with length, turn, and congestion costs, and support rip-up-and-reroute.
- Synthesize one logical network per compatible commodity and transport kind before embedding it into the grid. Shared trunks and trees are preferred; splitters and convergers are derived from the embedded result.
- CP or SAT solving is a local oracle. Approved uses include port assignment for a bounded facility group, local facility relocation, local packing, and proof that a precisely defined local neighborhood is impossible.
- A local proof must not be reported as proof that the whole factory request is infeasible.
- Improvement passes operate transactionally. Keep the current valid layout unless a candidate validates and improves the configured score.
- In the user-approved blueprint-edit experiment, a private improvement draft may retain overlapping facilities and pending original connections across multiple actions. Freeze the original directed topology, exact endpoints, rates and identities; reject rewiring before mutation. Supply that blueprint and the draft to the policy. Export or replace the best only after complete physical validation, and return the retained valid best when a draft is interrupted. This exception does not change construction or authorize invalid production output.
- Preserve the score priority: used bounding-box area first, physical belt and pipe tile count second, total route turns third, followed by later tie-breakers.
- The initial constructor may be visually poor. Every committed growth state must be valid for its covered production subgraph, and its final acceptance criterion is a complete validated factory at whatever natural footprint construction produces.
- A construction failure is a structured planner failure, not a global infeasibility proof. Diagnostics must identify the failing stage, network, facility group, or exhausted bound.
- Keep the former global exact joint solver as isolated research tooling. Production APIs and normal CLI commands must not invoke it, automatically fall back to it, or expose its timeouts as production planner failures.
- Before committing each planner slice, compare it against `docs/designs/2026-09-02.00-constructive-layout-planner.md` and this policy.
- The user-approved placement-only visual diagnostic may place all facilities without routing, outside production commands. It must label the output as unvalidated for factory use, preserve a frozen geometry artifact for a later fixed-placement routing probe, and never become a production fallback. Belt-first visual orientation/traversal in this diagnostic does not change production pipe-first routing.

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

- Keep verification proportional to a concrete failure mode and the change's impact. Reuse existing evidence and run the smallest sufficient checks. Do not add redundant tests, repeat successful checks without new cause, or broaden verification merely for reassurance.
- Keep expensive verification out of normal execution and performance-critical paths. Avoid repeated proof/replay work, duplicate computation, full source or artifact rehashing, and whole-directory scans in hot loops. Validate stable inputs at an appropriate boundary and reuse the result while their required identity remains valid. Retain checks necessary for input rejection, required invariants, resource limits and committed-output validity; improve their placement or implementation rather than weakening the contract.
- Use performance-affecting verification only when its necessity is established, as an explicitly invoked, bounded, one-off diagnostic. State the question it answers and its stopping condition. Expensive audits must not become implicit per-step, per-batch, periodic or normal completion work, or silently repeat for every production/training job.
- Isolate such diagnostics from long-running training and other normal workloads. Use a separate command/process and temporary configuration; do not attach costly inspection to an active long run or leave diagnostic modes, hooks, workers or schedules enabled afterward. Before a long run, verify that its actual launch path uses normal settings and that one-off diagnostics have ended. Include representative runtime measurements when checking that diagnostic work is absent from the normal path.
- A verification requirement is not permission to rerun the workload unnecessarily. Prefer retained evidence and targeted replay in the isolated diagnostic. Once the relevant checks pass, proceed with the task; add further verification only for a new change, failure or specific unresolved concern.
- Run `node docs/maintenance/check-tracked-files.mjs --worktree` after artifact or documentation changes and `node docs/maintenance/check-tracked-files.mjs --index` after staging, before committing. The index check also catches force-added ignored outputs and oversized staged content.
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
