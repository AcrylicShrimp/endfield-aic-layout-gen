# Bounded geometry-driven group translation

## Stage contract

This experimental improvement operator starts with a complete valid witness, runtime
catalogs and normalized graph. It proposes one rigid translation of a bounded facility
group, routes the resulting affected interfaces, and keeps only full-factory validated
area-first improvements. It is not a facility-only constructor or global placement model.

`closeGeometry(witness, seedIds, dx, dy, {maxMembers})` grows a seed monotonically,
with at most sixteen facility members. It includes owners needed to avoid translated
bodies overlapping retained bodies or occupied facility terminals, translated occupied
terminals overlapping retained bodies or same-layer occupied terminals, and partial
motion of non-detachable shared terminals. Recognized detachable zero-segment pairs
do not force shared ownership. External terminals are immutable in this neighborhood.
Every added member records the triggering facility/terminal and contact reason.

Inputs require a nonempty unique known seed, nonzero integer translation and member
budget 1..16. Output is `closed` with sorted member IDs and addition records, or
`rejected` with a structured diagnostic: code, info severity, stage `geometric-closure`,
message and entity. Codes distinguish input error, member budget and fixed external
contact. A closure is not a feasible route, a minimal feasible factory group, or proof
that no other port, orientation, placement or translation can work.

## Proposal portfolio and remaining checks

`pushCandidates(detail, graph, catalogs, options)` interleaves logical connected groups
and singleton seeds. Per seed it proposes cardinal shifts of one through eight cells
and corner alignments into the eight largest empty-rectangle diagnostics, interleaving
the two families so short pose budgets can reach both. These are
heuristic proposals, not an exhaustive placement domain. At most 4,096 raw poses are
examined by default (configurable 1..65,536). Candidate output limit is 1..512.
Every sampled pose is charged, including duplicate or rejected closures. All geometry
must remain in the explicitly retained witness extent; it is not a production W/H cap.

After closure, release only bridges incident to member interfaces or covered by moved
bodies. More than eight requested releases rejects that proposal; never truncate it.
Use the read-only all-bridge topology index for incident discovery. Other components,
carried routes, ownership and endpoint collisions remain subject to `movementPlan`.
This first operator does not recursively absorb blockers of rigid carried routes or
invent movement of external terminals and nonterminal components. Such rejections
remain visible and motivate a separately defined larger neighborhood if useful.

Candidates record translation, seed, closure additions, release IDs, body envelope and
endpoint-distance change. Rank by body-envelope area, then endpoint distance, with
per-seed interleaving before deeper variants. Real selection still uses validated
used area, physical transport cells and turns, in that order. Shortlisting never
declares omitted layouts invalid.

The explicit runner option is `--move-family push --bridge-policy release`, in move
or joint mode. Its existing routing orders, vacant-port retry budget, compaction and
independent graph validator remain unchanged. Reports include pose/member bounds,
closure/rejection counts, structured diagnostics and source hashes. No new production
fallback, solver dependency or game data is introduced.

## Verification gates

Test body-push cascades, both terminal/body directions, same-layer terminal contacts,
layer independence, non-detachable shared ownership, detachable pairs, fixed externals,
membership exhaustion, invalid IDs/translations and immutability. Verify deterministic
pose budgets and output bounds. Compare the operator against existing group families
on the same complete incumbent; inspect native full and focused results, and validate
original graph margins and crossing controls before selecting any improvement.

## Envelope-support seed experiment

`--push-seeds contact|boundary` selects the contact-only control (default) or adds
facility-envelope support cohorts. This flag is valid only for push mode. For each
of the four body-envelope sides, collect facilities reaching within one through four
cells of that side. Deduplicate identical membership sets, skip oversized seeds with
an explicit `push-boundary-seed-budget` diagnostic, and subject every remaining cohort
to the same closure, routing, release and full-factory validation gates. The four-cell
band is a proposal budget, not a canonical gap or a game-rule pruning premise.

`boundaryCohorts(witness, {maxMembers, bandLimit})` returns seeds (instances, side and
band) and oversized-cohort diagnostics. Tests use non-origin and unequal-size bodies;
the helper reads actual geometry and never labels recipes/facilities specially. It
does not include or move external terminals just because they define the used bounds.
Body-envelope support is only a seed heuristic; actual factory dimensions still include
all transport and logistics geometry. Reports retain original seed IDs/kind so a
boundary-cohort success can be distinguished from ordinary contact-push motion.
