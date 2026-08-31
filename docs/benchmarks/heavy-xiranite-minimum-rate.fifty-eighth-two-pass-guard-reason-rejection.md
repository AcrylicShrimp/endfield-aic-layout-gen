# Two-Pass Guard-Reason Optimization Rejection

## Question

The grouped guarded-item propagator constructs a provisional disequality reason while scanning for
positive item support. When a later item supports the relation, that provisional reason is thrown
away. This experiment tested whether separating support detection from reason construction would
reduce the propagator's hot-path cost.

The candidate implementation performed:

```text
first pass:  find any common positive item without constructing a reason
second pass: only after disjointness, construct the complete reason
```

This changes neither the exact model nor the inference. Domains are read through one stable
`PropagationContext`, and the second pass emits the same one-predicate-per-item explanation.

## Evaluation contract

The grouped result in
`/tmp/aic-guarded-intersection-grouped-final.uEK8Td/summary.json` is the baseline. The final
two-pass run is:

```text
/tmp/aic-guarded-intersection-two-pass-final.1dcgem/summary.json
```

Both use the Heavy Xiranite cumulative Phase 0-2 sweep, 12 workers, and an independent five-second
search budget per fixed-dimension candidate. A preliminary run showed the same no-gain direction
but contained an inaccurate release-only membership-check diagnostic; it is excluded from the
tables. The diagnostic was corrected and independently reviewed before the final run.

## Result

| Phase 2 metric | Grouped baseline | Two-pass | Change |
|---|---:|---:|---:|
| Proven infeasible | 39 | 39 | 0 |
| Unknown | 61 | 61 | 0 |
| Feasible | 0 | 0 | 0 |
| Outer wall | 36,886 ms | 37,015 ms | +0.3% |
| Summed search | 335,585 ms | 336,443 ms | +0.3% |
| Summed construction | 60,265 ms | 61,313 ms | +1.7% |
| Membership checks | 2,153,554 | 2,734,358 | +27.0% |

The aggregate search-prefix counters are not directly comparable because earlier non-binding hints
and timeouts can change the explored prefix. The completed proof cases are the causal comparison:

| Dimensions | Search tree | Grouped | Two-pass | Change |
|---|---|---:|---:|---:|
| 12x9 | 1,057 decisions / 15 conflicts | 1,197 ms | 1,193 ms | -0.3% |
| 13x9 | 1,581 decisions / 18 conflicts | 1,648 ms | 1,647 ms | -0.1% |
| 14x9 | 2,192 decisions / 21 conflicts | 2,169 ms | 2,212 ms | +2.0% |

Native solver propagation counts are identical within each row. Avoiding transient reason
allocation does not produce a measurable gain, while disjoint pairs require a second domain scan.
The additional 580,804 membership checks erase the expected benefit.

The whole parallel run's maximum resident set size changed from 5,197,627,392 to 5,179,424,768
bytes. This 0.35% difference is below what can establish an allocation or memory improvement for
short-lived small vectors.

## Independent review resolution

Three independent reviews confirmed solver soundness, stable-domain reasoning, complete reasons,
and unchanged wake/backtrack behavior. One reviewer found that the first draft counted a
debug-only verification lookup in release diagnostics. The lookup and its counter increment were
gated together, after which all reviewers returned PASS.

The performance acceptance gate was not met. Both completed-case and end-to-end measurements show
no repeatable speed improvement.

## Decision

Reject and revert the two-pass implementation. Commit `162f414` remains the authoritative grouped
guarded-item baseline. This negative result closes the current guarded-item micro-optimization
cycle; further local polishing is not justified by its measured benefit.

The next research slice returns to Phase 2 cliff diagnosis. It should first re-establish the current
hierarchy of placement, port, terminal, and routing couplings, then test the strongest remaining
candidate rather than assuming it. The known weak placement-port-geometry `Element` channel is the
leading candidate for a semantics-preserving reformulation experiment.
