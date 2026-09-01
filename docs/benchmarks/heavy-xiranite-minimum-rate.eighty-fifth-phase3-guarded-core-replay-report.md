# Heavy Xiranite Phase 3 Guarded-Core Replay

## Result

The proof-shrunk nine-predicate core can be replayed soundly as one exact clause in the ordinary
Phase 3 joint placement-and-routing model, but it does not change the unrestricted-root outcome or
any of the nine observed retained-atom domains in this five-second experiment.

All four counterbalanced authoritative runs and both independent observation runs terminate
`Unknown`. The control and replay models have identical 64,471 variables. Replay adds exactly one
nine-literal clause, nine terms, and nine factor-graph incidences. All proof, certificate,
unrestricted-boundary, model-identity, exact-delta, root-snapshot, and evidence gates pass. The
experiment is `Completed`, not blocked, and reports `NoOutcomeWinner`.

The clause changes none of its nine native root domains and does not create a root conflict. Small
counter reductions are visible, but two censored samples per authoritative arm are not enough to
claim a performance improvement. The correct interpretation is a clean negative result for global
root conflict and retained-atom pruning, not a whole-model root-domain census or a rejection of
exact guarded feedback.

## Controlled Contract

The replay target is the ordinary cumulative Phase 3 feasibility model under the fixture's
`16 x 16` ceiling. Placement, rotation, ports, external boundary endpoints, routing, and used
dimensions remain solver decisions. The fixture ceiling is not a project or game invariant.

The experiment executes fresh solver instances in counterbalanced order:

```text
A: baseline authoritative
B: replay authoritative
B: replay authoritative
A: baseline authoritative
baseline root observation
replay root observation
```

The baseline posts no guarded predicate. Replay posts exactly:

```text
not(all nine proof-shrunk core predicates)
```

The clause uses the native complements certified by the independently proven infeasible
conjunction. It introduces no selector or auxiliary variable. A prior exact solution is supplied
only as a non-binding hint and does not satisfy the complete replay conjunction.

## Outcome Summary

| Run | Outcome | Search | Decisions | Backtracks | Conflicts | Learned clauses | Solver propagations |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| A-B baseline | Unknown | 5,012 ms | 6,991 | 183 | 182 | 182 | 2,448,471 |
| A-B replay | Unknown | 5,013 ms | 6,604 | 165 | 164 | 164 | 2,332,264 |
| B-A replay | Unknown | 5,012 ms | 7,175 | 190 | 189 | 189 | 2,497,323 |
| B-A baseline | Unknown | 5,012 ms | 7,547 | 222 | 221 | 221 | 2,625,626 |
| Baseline observation | Unknown | 5,012 ms | 7,888 | 251 | 250 | 250 | 2,775,867 |
| Replay observation | Unknown | 5,011 ms | 7,584 | 225 | 224 | 224 | 2,634,115 |

Authoritative A/B means are:

| Counter | Baseline mean | Replay mean | Observed delta |
| --- | ---: | ---: | ---: |
| Branch decisions | 7,269.0 | 6,889.5 | -5.2% |
| Backtracks | 202.5 | 177.5 | -12.3% |
| Conflicts | 201.5 | 176.5 | -12.4% |
| Learned clauses | 201.5 | 176.5 | -12.4% |
| Solver propagations | 2,537,048.5 | 2,414,793.5 | -4.8% |

These deltas are advisory. Every run consumes the complete five-second budget, no run reaches a
terminal result, and there are only two counterbalanced authoritative samples per arm. They do not
authorize a winner or an architecture decision.

## Exact Model Delta

| Metric | Baseline | Replay | Delta |
| --- | ---: | ---: | ---: |
| Variables | 64,471 | 64,471 | 0 |
| Constraints | 163,815 | 163,816 | +1 |
| Constraint terms | 635,617 | 635,626 | +9 |
| Factor-graph constraint vertices | 163,815 | 163,816 | +1 |
| Factor-graph incidences | 626,017 | 626,026 | +9 |
| Cross-family constraints | 152,630 | 152,631 | +1 |
| Placement-routing constraints | 48,618 | 48,619 | +1 |
| Placement-routing incidences | 244,622 | 244,631 | +9 |

The one new constraint family is `guarded-core`, with one arity-nine relation and coefficient
magnitude one. The recorded base model, aggregate variable and per-family domain metrics,
non-guarded constraint families, and incidence vectors are identical. The baseline also exactly
matches the accepted initial atom-free control model under those recorded checks.

## Root Effect

| Metric | Result |
| --- | ---: |
| Baseline root conflict | No |
| Replay root conflict | No |
| Newly root-eliminated | No |
| Changed retained-atom domains | 0 / 9 |

The two placement literals remain Boolean, the five facility-port literals retain their complete
declared domains, and the two selected material-arc literals remain Boolean. This is expected for a
non-unit nine-literal clause while all high-level and route decisions are still free: the clause
cannot prune until enough of its literals become true.

## Validity Gates

All controlled-experiment gates pass:

- the source shrink completed with an independently proven nine-predicate infeasible conjunction;
- the retained and removed atoms form an exact, distinct partition of the canonical 30 atoms;
- every replay atom resolves freshly to the expected native domain and predicate complement;
- all ten external terminals retain their unrestricted sparse legal domains;
- control and replay share the same workload, phase, ceiling, hint, and base-formulation semantics;
- replay differs by exactly one certified clause and its nine terms and incidences;
- all six outcomes are free of invalid witnesses and cross-run satisfiability contradictions;
- root observation is captured and agrees with the authoritative interpretation;
- repeated A-B-B-A outcomes are consistent.

During independent review, a fail-closed reporting defect was found and corrected. A run with a
certificate, model, boundary, delta, root, or evidence failure could previously be `Blocked` while
still naming a performance winner. Performance comparison now requires the complete interpretation
gate, and invalid experiments serialize `InconclusiveInvalidExperiment`. Separate regression tests
also prevent an intervening `Unknown` from hiding feasible/infeasible evidence conflicts, preserve
root-conflict evidence, and verify all six artifact names.

## Interpretation

The replay clause is sound but creates no root conflict and changes none of its nine observed atom
domains in the unrestricted model. It has not yet failed the intended progressive-decomposition use
case. A decomposition loop would replay the cut after some high-level placement, port, or boundary
choices are fixed; in that smaller child, the same clause may become unit or conflicting
immediately.

Therefore the next question is not whether another unrestricted five-second sample produces a
slightly different counter. It is whether this exact clause rejects multiple distinct high-level
assignments inside the predeclared predecessor subspace.

## Next Exact Experiment

First certify the breadth universe. The accepted chain has 16 exact residual facility-port tuples,
but the 54 observed boundary keys belong only to the previously selected tuple. The unrestricted
sparse legal domain contains 544 keys, and changing a port tuple may change which keys survive root
propagation. Therefore `16 x 54 = 864` is not yet a certified Cartesian portfolio.

Run a fresh control root-domain census for each of the 16 tuples. For tuple `t`, record the exact
root-live key set `K_t` for the selected external terminal, together with the unrestricted 544-key
certificate, root status, fixed-port certificate, and base-model identity. Then define the finite
exact predecessor universe as:

```text
U = { (t, k) | t is one of the 16 certified tuples and k is in K_t }
|U| = sum over t of |K_t|
```

The census has an explicit fail-closed root-status branch:

- after a normal root capture, `K_t` is the complete observed key domain;
- if propagation certifies root infeasibility before the first decision, `K_t` is the empty set and
  the report preserves the raw proof/status plus complete fixation, model, and boundary
  certificates;
- a missing snapshot without certified root infeasibility, invalid witness, observation timeout,
  proof conflict, or incomplete certificate blocks the whole census.

No tuple may be omitted silently or supplied with a fabricated empty snapshot. Consequently
`sum |K_t|` excludes only tuple slices proved empty at root.

Only after that census, execute a fresh control root-only build for every member of `U`, finish the
complete control wave, and then execute the complete replay wave. Each pair must differ by exactly
the one nine-literal clause, with no solver reuse, wave overlap, observer feedback, inherited
separator, junction, continuation, or route-leaf restriction. If and only if all 16 tuple censuses
independently expose the same exact 54-key set may the run certify the earlier 864-case count.

For every case, record all nine literal states at root and classify the clause as satisfied,
unresolved, unit, or conflicting. Also record control liveness, high-level projection match,
new root elimination, route-only guarding, model identity, and certificate validity. Fail closed on
any missing or duplicate case, model delta, invalid result, or contradictory pair.

This data-dependent portfolio is exact only inside the accepted fixed-placement, `16 x 16`
predecessor subspace. It does not measure global Phase 3 coverage. A fixed 864-case run without the
per-tuple census would be a sample and cannot authorize a negative `STOP` conclusion.

The accepted decision rule remains:

- fewer than two matched control-root-live assignments: `STOP`;
- at least two matched and at least two newly root-eliminated assignments: `GO` for only a bounded
  two- or three-core cumulative replay prototype;
- multiple matched but zero or one newly eliminated assignment: `STOP` the current core escalation
  and examine a higher-level routing-infeasibility cut;
- any completeness, proof, certificate, model, or evidence failure: `BLOCKED` with no strategy
  conclusion.

## Improvements Preserved To Date

The path to this experiment has preserved the following exact improvements:

1. sparse shared belt and pipe layer state instead of one dense grid per logical line;
2. independent placement and port variables with explicit bidirectional channeling;
3. shared physical occupancy propagation between facility footprints and both transport layers;
4. exact sparse external terminals inside the shared routing model instead of heuristic connector
   shapes;
5. stronger exact port-element and endpoint-continuation propagation;
6. possible-graph connectivity, watched-demand unique-support propagation, local continuation, and
   guarded positive-item intersection;
7. exact dimension portfolios and complete diagnostic counters for decisions, backtracks,
   conflicts, learned clauses, and propagations;
8. systematic Phase 3 cliff decomposition down to a certified 30-predicate infeasible leaf;
9. proof-only sequential shrinking from 30 predicates to an independently proven nine-predicate
   core;
10. exact zero-variable replay as one certified nine-literal clause with counterbalanced A/B and
    independent root observation.

None of these measurements authorize a heuristic restriction or production architecture cutover.

## Runtime And Artifacts

| Metric | Result |
| --- | ---: |
| Replay A/B and observation stage | 33,931 ms |
| Complete parent, shrink, and replay chain | 584,222 ms |
| Artifact size | 187 MiB |

Release artifacts are outside the repository at:

```text
/tmp/aic-guarded-core-replay.2frSjZ/summary.json
/tmp/aic-guarded-core-replay.2frSjZ/summary.html
/tmp/aic-guarded-core-replay.2frSjZ/ab-0.baseline.authoritative.html
/tmp/aic-guarded-core-replay.2frSjZ/ab-1.replay.authoritative.html
/tmp/aic-guarded-core-replay.2frSjZ/ba-0.replay.authoritative.html
/tmp/aic-guarded-core-replay.2frSjZ/ba-1.baseline.authoritative.html
/tmp/aic-guarded-core-replay.2frSjZ/baseline.observation.html
/tmp/aic-guarded-core-replay.2frSjZ/replay.observation.html
```

Verification for this slice:

```text
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
cargo build --release -p aic-cli --bin aic-prior-terminal-pair
```

The release runtime artifact was generated with the same fixture and budgets documented by the
preceding report, plus:

```text
--guarded-core-initial-gate
--guarded-core-full-time-limit-ms 5000
--shrink-guarded-core
--guarded-core-shrink-time-limit-ms 5000
--replay-guarded-core
--guarded-core-replay-time-limit-ms 5000
--output-dir /tmp/aic-guarded-core-replay.2frSjZ
```

The complete exact invocation, including every parent-chain flag, remains recoverable from this
slice's execution log but is not embedded in the runtime artifact itself. Future diagnostic
artifacts should serialize their normalized CLI request so reproduction does not depend on the
execution log.
