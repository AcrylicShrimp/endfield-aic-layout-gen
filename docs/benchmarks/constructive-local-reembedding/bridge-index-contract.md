# Bridge topology indexing versus local release budgets

## Measured failure and contract

A complete validated compound relocation reduces the incumbent to 44x54, with twelve
bridges. The next round aborts while constructing an index because it requests release
of every bridge through the eight-release transaction API. Eight is a local mutation
budget, not a maximum bridge count in a factory or an indexing ceiling.

`bridgeTopologyView(witness, catalogs)` is a read-only topology projection. It checks
every existing bridge against the same catalog-backed, terminal-free, straight,
equal-rate two-owner rule used by release, then returns a private witness view with
bridge component anchors suppressed and the corresponding transparent cell set.
Its cost is bounded by the supplied witness size, not the local release budget.
It preserves every network's terminals, cells and segments, other components and the
input witness. This is an index input, NOT a candidate or valid factory geometry.

`releaseBridgeDraft(witness, catalogs, ids)` remains limited to at most eight unique
existing bridge IDs per transaction. Both operations share one internal structural
projection implementation, with separate public purposes and budget checks. The
production routing/validator path does not gain an unbounded release operation.

Coalescence and the experiment runner use the all-bridge view only to derive terminal
relationships and incident bridge IDs. Actual candidates still explicitly request
their incident release set. A coalescence pair requiring more than eight releases
is skipped with `local-bridge-release-budget`, recorded as an info diagnostic at
stage `bridge-release` with pair and bridge references, plus a rejection count.
Other candidate families' over-budget requests stay ordinary per-attempt rejections.
No candidate's release set is silently truncated. The complete incumbent survives
an exhausted local neighborhood.

## Verification

Regression: topology/coalescence indexing supports nine or more valid bridges while
an explicit nine-bridge release still rejects. Invalid bridge ownership/rate/terminal
geometry still rejects in either entry point. An over-budget incident coalescence
pair reports its bound without aborting other pairs. Indexing is immutable and
deterministic. Rerun the actual twelve-bridge continuation, independently validate
accepted results, and inspect native full/focused renders before selecting a best.

This is a diagnostic-contract correction, not a change to game crossing rules or a
claim of local/global infeasibility. Production construction is unchanged.
