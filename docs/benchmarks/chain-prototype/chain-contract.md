# Milestone 1: generated routed chains

Status: implemented; see `chain-report.md`. Separate experimental Node CLI; no production API changes.

## Input DTO

Schema 1 contains `id`, `nodes`, `edges`, `external`, `policy`, and `budget`, with no additional
fields. A node has `id`, `recipe`, `utilization`. An internal edge has `id`, `source`, `target`,
`item`, `rate`. An external obligation has `id`, `instance`, `direction`, `item`, `rate`.
All rates are positive exact rationals. Runtime catalogs supply facilities, ports, allowed
rotations, recipes, items, transports, and components. The full graph ledger must exactly
equal every utilized recipe input/output margin; external obligations are never inferred to
hide a missing internal edge. No poses, port IDs, route corners or canvas dimensions are inputs.

Milestone 1 supports a connected directed simple chain of 2–16 facilities with one internal
edge between each neighboring pair, no self-edge, cycle, branch, merge or parallel edge.
Every edge fits one transport-capacity lane. A chain must not require adding a pipe after
a belt during sink-first growth; unsupported mixed ordering is rejected explicitly until a
later network-growth stage can handle it. External requirements remain declared pending
obligations: success is an internally validated chain, not a full external factory.

`policy` contains distinct `families` drawn from `straight`, `left`, `right`, and distinct
positive integer `gaps` (bounded diagnostic proposal intervals, not mandatory game spacing).
It also contains `ports_per_side`, the number of center-ranked port choices to try. A family
is a proposal strategy, not a pose hint or rule that omitted solutions are illegal. The sink
starts in a catalog-permitted orientation with west-facing incoming ports when available;
the other orientations remain an explicitly unimplemented proposal dimension in this slice.

`budget` contains positive integer `proposals`, `route_expansions`, `validations`, and
`elapsed_ms`. These are shared across all families, not reset for each full-chain trial.
Candidate generation charges each port/rotation/gap pose considered, including rejected ones.
Search expansions and validation calls are charged before selection. A deadline is checked
between operations and caps each independent validator process. The existing router's finite
grid guard is diagnostic, not a blueprint size constraint.

## Construction and proposal contract

Derive the unique sink-first order from graph connectivity, independent of node/edge array
order. Validate a seed witness before considering any upstream placement. For each family,
grow one predecessor at a time. Generate centered and nearby port choices from actual catalog
geometry. A straight proposal faces the supplier output toward the consumer input; left/right
proposals rotate that approach by a quarter turn and put the supplier beyond the corresponding
consumer body edge. Try the configured gaps without automatically contracting deliberate slack.
Left/right describe the sink-to-source growth traversal, not the opposite material-flow traversal.

Each candidate clones the retained witness, adds only the new predecessor, extends the
item/transport logical network, and routes that edge immediately through the dedicated A*
engine. Facility bodies block both layers; existing transport blocks only its own layer.
The simple-chain slice has no shared junctions or derived components; unsupported multi-lane
or branching input is a structured input limitation, not a fallback to isolated edge routing.
One network DTO groups all compatible same-item/same-kind requirements before physical routing.

Canonical translation may shift the entire witness to nonnegative coordinates. It must preserve
all older relative geometry, route directions, terminal identities, utilization and rates.
The independent validator and a graph-derived pending-margin check must pass before committing
the new facility. A rejected candidate cannot change the parent. A validation or budget failure
cannot authorize expanding an unvalidated state. No all-facility-only placement phase exists.

Each family selects a currently validated candidate by area, physical transport cells, then
segment-only turns, and stable generation order. This intentionally exposes a partial-state
baseline; completion-aware lookahead and rollback are later milestones. Retain each family's
completed chain separately. Compare only fully covered chains with the same inventory, never
a smaller failed prefix against a completed candidate. Preserve prefixes for diagnostics.

## Output and failure DTOs

CLI flags: `--request`, explicit catalog flags, `--validator`, `--output-directory`.
Outputs: an input-provenance report; per-validation immutable witness, independent report and
process log; per-family growth decisions and accepted-state identities; the selected internal
chain witness, score and HTML; or a structured stopped report with retained valid prefixes.
The report has `schema_version`, `status`, `factory_validated: false`, family outcomes, budget
limits/usage, decisions, selected artifact identity, and diagnostics. Every decision records
family, growth index, facility, ports, rotation, gap, pose, routing result/work, validation
artifact, score when valid, and outcome. Complete-family comparisons are internal-only.

Stable error categories use prefix `chain-`: input-invalid, unsupported-topology,
unsupported-lanes, unsupported-pipe-order, proposal-overlap, route-failed, validation-failed,
margin-mismatch, budget-exhausted, and no-complete-family. Each has severity, stage, message,
and an entity reference. Router failure includes the local bounds and termination code; it
does not prove global infeasibility. An exhausted run returns its existing valid states.

## Verification gates

- Actual catalog-backed two- and three-facility chain fragments construct in all three families.
- Each accepted history grows by exactly one routed facility and passes independent validation.
- Straight/left/right and gap alternatives are visible in the generated pose/route trace.
- Same request and work limits reproduce geometry and deterministic work counts (except time).
- Renamed graph IDs and reordered arrays do not change selected geometry.
- Bad ledgers, fixed geometry, cycles/branches, over-capacity lanes and mixed-order violations
  fail explicitly. Tight proposal/search/validation bounds preserve validated prefixes.
- No production constructor, exact solver, screenshot transcription or paused breadth harness
  is imported by the constructor. Neutral geometry, rate, score, A*, validation and rendering
  utilities may be reused without adapters to old construction orchestration.
- CLI artifact checks, unit/integration tests, workspace verification, and a rendered inspection
  precede a focused milestone commit. This is not completion of the full prototype goal.
