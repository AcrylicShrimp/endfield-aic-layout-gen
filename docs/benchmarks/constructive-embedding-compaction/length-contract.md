# Minimum chain length at the minimum axis extent

First solve the existing fixed-order difference constraints for minimum extent. Fix the low/high
boundaries at zero and that minimum span. Bound every rigid group's coordinate by the span minus
its largest point offset, and propagate upper bounds through reversed difference inequalities.
The least feasible solution remains a feasibility certificate; no caller canvas is introduced.

Suppress the same constant-rate straight interiors used by geometry lowering. Deduplicate physical
parallel chains by transport layer and unordered geometric endpoints. Their orientations are fixed,
so length is a linear integer cost in rigid-group coordinates and offsets. Perpendicular segments
and critical-point coincidences stay fixed. Unique physical tile count differs from the sum of
these lengths by a fixed topology constant. Verify the predicted change against independently
counted actual physical tiles after lowering; a mismatch is a failed certificate, not an accepted
minimum-tile claim. Rates are never cost weights and are never modified.

For each bounded integer variable X in [L,U], introduce thresholds T(k) = [X >= k], L < k <= U.
Enforce T(k+1) implies T(k). X[v] >= X[u] + w becomes T[u,k] implies T[v,k+w]; thresholds already
guaranteed by lower bounds need no node. Bounds are feasible extremal solutions, so implications
beyond the target upper bound must not arise. Costs are c per selected threshold. Positive costs
connect to the sink, negative costs from the source; implications use capacity greater than the
sum of all finite capacities. A minimum cut solves this finite integer closure problem exactly.

Return explicit bounds, coefficients, selected coordinates, graph/work counts, max-flow value and
matching primal/dual integer cost. Check every bound, original difference inequality and cost
equality. Bound nodes, edges and flow work explicitly; exceeding a diagnostic budget rejects the
candidate without replacing the validated incumbent. No relaxed constraint, heuristic fallback,
new dependency or global optimality claim is introduced. This is secondary optimality for the
same fixed-perpendicular-coordinate/order/coincidence/topology neighborhood only.

Use iterative flow traversal rather than stack-depth-dependent recursion. Independently enumerate
tiny bounded systems and compare their minimum cost, including negative coefficients/weights,
zero-cost ties, fixed variables, resource failures and reverse chains. Run complete-factory audits
and inspect actual rendered before/after geometry before accepting the slice.
