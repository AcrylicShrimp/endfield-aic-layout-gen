# Bounded private-seed lookahead portfolio

## Input and output

`lookahead-run.mjs` receives the usual explicit graph/catalog/validator paths, a
`--baseline` global-incumbent pointer, a `--seed-report` schema-4 local-run report,
`--seed-limit 1..4` (default 2), `--rounds 1..3` (default 3), `--workers 1..4`
(default 2), and a new output directory. The seed report must have matching current
graph/catalog/validator and engine provenance. Do not silently replay stale routing
IDs or different game data. Every referenced witness is read-only.

Only `unimproved-validated` block-transform audit records are seed candidates.
Recompute their scores from witness geometry, deduplicate exact witness content,
sort by area/cells/turns and then source stage, and select at most the requested
seed limit. The ordering is deterministic, not based on process completion order.
The global baseline and every selected seed must pass fresh independent geometry
and original-graph validation before use. Claimed report validation is not trusted.

The output report records global root, candidates/seeds and their hashes, selection
and deduplication counts, branch process results, private scores, final validation,
global comparisons, termination, worker/round/seed budgets, elapsed time and complete
provenance. Diagnostics carry stable code, severity, stage, message and an entity.
The selected-directory pointer always identifies a complete validated global factory.

## Private branch contract

Each seed is a private complete factory, not a replacement for the global root.
Evaluate branches concurrently up to the worker limit, each through the existing
joint boundary-push runner with a 512-position limit, requested round cap, existing
endpoint/junction/crossing/port/layer policies and at most four extra unimproved
audits. Preserve all existing per-attempt movement and routing bounds. The child
does not start another lookahead. Branch-local improvements do not imply global
improvement.

Each branch owns a POSIX process group. A 180-second timeout sends SIGTERM to that
owned group, then SIGKILL after a one-second grace period; if the main process closes
during timeout cleanup, still kill any surviving group members. The driver retains
the process handle until close. Report spawn failure, timeout, missing/invalid child
report or validation rejection explicitly and keep the validated global root.
Unsupported platforms fail before output creation; this is experimental POSIX tooling.

Freshly validate every successfully returned branch factory against the original
graph. Compare results with the immutable global root, then select the best complete
validated result with stable seed ordering for ties. A valid seed already better than
the root may also be considered, but must not bypass fresh validation. Publish a new
global selection only after this comparison. No error or partial file may replace it.

## Verification and limits

Tests cover score recomputation, exact deduplication, stable selection, invalid budgets,
concurrency bounds, failed/private-worse candidates and area-first global acceptance.
Process tests cover success, spawn failure and forced timeout of an owned process
group. Manually replay the proven F30/F44 area-reduction case, then exercise current
incumbent seeds. Independently audit and visually inspect any new global factory.
`lookahead-audit.mjs --seed-report <local-report> --output-directory <new-directory>`
checks CLI budget/provenance rejection, empty and missing seed retention, and fresh
rejection of forged seed/root validity against the actual original graph.

This is a bounded seed portfolio, not exhaustive search, a global optimality proof,
automatic promotion of a worse incumbent or a production fallback. It formalizes the
prior explicit lookahead control without changing the local pose/routing contract.
