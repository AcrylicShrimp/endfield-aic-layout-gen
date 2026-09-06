# Alternate optimal closures and private axis-pair transactions

The existing bounded minimum-closure oracle returns a least coordinate optimum. Add a greatest
coordinate optimum without changing the minimum extent, linear cost, bounds or inequalities.
After maximum flow, the least source side is the set reachable from the source in the residual
graph. The greatest source side is the complement of vertices that can reach the sink. Any
minimum cut has no outgoing residual arc, contains the source and excludes the sink. Thus it
contains the first set and excludes the second; both extremal sets themselves are minimum cuts.
Threshold prefixes turn set inclusion into componentwise coordinate order. Check the same primal/
dual objective and every geometric constraint for either choice. These are extrema, not an
enumeration of all optimal cuts. Reverse traversal and decoding count against the work budget.

One private transaction starts from a complete validated incumbent and applies an axis followed
by the perpendicular axis. Try both extremal cuts at each step. An equal-score first step is a
legal private intermediate; do not discard it before trying the other axis. Independently validate
every emitted witness. Only a complete strictly better final or prefix result replaces the public
incumbent. Keep prefix/endpoint parent references, cut policy, scores, certificates and rejection
diagnostics in the report. Compare both axis orders in the experiment, preserve prior baselines,
and inspect the actual rendered result. Resource limits and no improvement are not infeasibility
or global optimality proofs. Explore unique equal-score witnesses across further pairs under
explicit round and processed-state budgets. Hash canonical witness semantics (including rates,
utilization, cells, segments and terminals), not incidental array order or diagnostic metadata.
Queue new validated witnesses at the best current score; higher-score branches are intentionally
discarded. Report processed/seen/deferred states and the exact stopping bound. Even exhaustion
of this operator-generated plateau is not exhaustion of every geometric neighborhood.

Verify extremal values against all optima of tiny integer systems, exact costs, prefixes, work
limits and detached candidate materialization. Test the orchestration case where an equal-score
intermediate improves only after the second axis and ensure invalid candidates never enter it.

Reuse independent validation only for an identical semantic witness within the same run and
catalog/graph context. Cache successful validations, never failures. Record the original validated
directory and the new generation certificate/timing on a reused attempt. Skip stale higher-score
parents before charging the state budget. Any rejected generation or validation distinguishes
diagnostic-limited exhaustion from ordinary exhaustion of these extremal operators.
