# Heavy Xiranite Phase 3 Guarded-Core Boundary Census

## Result

The exact residual-port census completed without an interpretation block. Twelve of the sixteen
certified residual facility-port tuples are inconsistent at root. The remaining four tuples each
retain the same 54 legal boundary keys for the selected external demand. The exact root-supported
predecessor universe is therefore 216 `(tuple, boundary-key)` pairs, not the previously assumed
864.

Two of the four root-live tuples are subsequently proven infeasible by the bounded search that
follows root capture. The other two remain `Unknown` at five seconds. These facts define two
different exact populations:

```text
U_root    = 4 root-live tuples x 54 keys = 216 pairs
U_search  = 2 not-yet-refuted tuples x 54 keys = 108 pairs
```

`U_root` remains the authoritative denominator for the next root-only guarded-clause breadth A/B.
`U_search` is a separate exact cohort for a later five-second search-performance experiment. A
nonempty root domain is propagation support, not a feasibility witness, so the later proofs do not
retroactively turn the 216 root-supported cases into 108.

## Controlled Contract

The census preserves the accepted fixed Phase 3 predecessor subspace:

- `16 x 16` used dimensions inside the request's `16 x 16` ceiling;
- three exact prior placements plus the selected fourth facility coordinate and rotation;
- eleven inherited fixed facility terminals;
- the exact Cartesian enumeration of four residual binary port domains, producing sixteen tuples;
- the sparse unrestricted legal-boundary formulation with 544 legal keys per external terminal;
- all placement-routing decisions not named above remain solver decisions;
- no guarded-core clause, separator, junction, endpoint-continuation, or other route-leaf predicate
  is posted.

For each tuple `t`, a fresh model captures the complete target-terminal boundary-key domain before
the first search decision. A normal capture defines `K_t`. A certified root conflict defines
`K_t = empty`. Missing, unrecognized, or invalid root evidence blocks the entire census.

Every case carries a build certificate independent of root capture. It records and validates the
dimensions, fixed coordinate and rotation, all fifteen sorted terminal-port requests, three prior
placements, prior-overlap fixation mode, sparse legal-boundary mode, and completed exact model
construction. Every case's model metrics, complete complexity metrics, and formulation are also
matched against and serialized with the accepted sparse predecessor reference.

## Tuple Census

| Tuple | Root status | `|K_t|` | Five-second outcome | Search | Decisions | Backtracks | Conflicts | Propagations |
| ---: | --- | ---: | --- | ---: | ---: | ---: | ---: | ---: |
| 0 | Proven root-infeasible | 0 | ProvenInfeasible | 57 ms | 0 | 0 | 1 | 168,902 |
| 1 | Proven root-infeasible | 0 | ProvenInfeasible | 55 ms | 0 | 0 | 1 | 168,903 |
| 2 | Proven root-infeasible | 0 | ProvenInfeasible | 63 ms | 0 | 0 | 1 | 175,676 |
| 3 | Captured | 54 | ProvenInfeasible | 1,677 ms | 14,952 | 1,503 | 1,504 | 1,514,503 |
| 4 | Proven root-infeasible | 0 | ProvenInfeasible | 60 ms | 0 | 0 | 1 | 168,903 |
| 5 | Proven root-infeasible | 0 | ProvenInfeasible | 56 ms | 0 | 0 | 1 | 168,904 |
| 6 | Captured | 54 | Unknown | 5,007 ms | 49,452 | 4,813 | 4,812 | 5,132,381 |
| 7 | Proven root-infeasible | 0 | ProvenInfeasible | 62 ms | 0 | 0 | 1 | 175,680 |
| 8 | Proven root-infeasible | 0 | ProvenInfeasible | 64 ms | 0 | 0 | 1 | 175,682 |
| 9 | Captured | 54 | ProvenInfeasible | 840 ms | 6,622 | 468 | 469 | 763,776 |
| 10 | Proven root-infeasible | 0 | ProvenInfeasible | 57 ms | 0 | 0 | 1 | 168,906 |
| 11 | Proven root-infeasible | 0 | ProvenInfeasible | 57 ms | 0 | 0 | 1 | 168,907 |
| 12 | Captured | 54 | Unknown | 5,006 ms | 53,305 | 4,646 | 4,645 | 5,139,079 |
| 13 | Proven root-infeasible | 0 | ProvenInfeasible | 44 ms | 0 | 0 | 1 | 175,686 |
| 14 | Proven root-infeasible | 0 | ProvenInfeasible | 44 ms | 0 | 0 | 1 | 168,907 |
| 15 | Proven root-infeasible | 0 | ProvenInfeasible | 45 ms | 0 | 0 | 1 | 168,908 |

The four captured sets are exactly equal. Tuple 6 is the predecessor selected by the earlier
boundary-key experiment and its 54-key set is reproduced exactly. The top-level
`all_root_live_sets_equal` and `all_sets_equal_selected_parent` fields are correctly false because
the twelve certified empty sets are included in those all-sixteen comparisons.

## Aggregate Runtime

| Metric | Value |
| --- | ---: |
| Fresh models | 16 |
| Workers | 12 |
| Budget per tuple | 5,000 ms |
| Model-construction sum | 13,921 ms |
| Mean model construction | 870.1 ms |
| Search sum | 13,194 ms |
| Root-infeasible search sum | 664 ms |
| Captured-case search sum | 12,530 ms |
| Parallel census wall time | 11,725 ms |
| Complete predecessor chain plus census | 594,685 ms |

The sixteen solves execute in two worker chunks, first twelve and then four. The cumulative chain
time is not the census cost.

## Model Scale

Every census case matches the same accepted sparse predecessor structure:

| Metric | Value |
| --- | ---: |
| Variables | 64,471 |
| Boolean variables | 58,918 |
| Integer variables | 5,553 |
| Log2 domain volume | 72,149.75 |
| Constraints | 163,836 |
| Constraint terms | 635,638 |
| Factor-graph incidences | 626,038 |
| Placement-routing constraints | 48,618 |
| Placement-routing incidences | 244,622 |

## Validity Gates And Review Fixes

All source-replay, exact tuple-enumeration, complete fixation-request, build-certificate,
selected-parent reproduction, census-completeness, model-identity, unrestricted-boundary, and
evidence gates pass. The final status is `Completed`, `interpretation_blocked = false`, and the
fixed-864 certificate is correctly false.

Three independent implementation reviews initially blocked the experiment and found three common
evidence defects:

1. uniformly drifted census models could agree with one another without agreeing with the accepted
   sparse predecessor;
2. a root-infeasible case did not preserve enough evidence that all dimensions, placements, and
   fifteen port requests reached the model builder;
3. a blocked census could still serialize `fixed_864_case_count_certified = true`.

The implementation now anchors every model to the accepted sparse predecessor, emits and validates
an independent native build certificate for every case, serializes expected and actual model
identity, accepts only the two root-snapshot statuses emitted by the brancher, and gates the 864
certificate on the complete nonblocked interpretation. Focused regressions cover unrecognized root
statuses, blocked 864 certification, and incomplete build certificates. All three reviewers passed
the corrected implementation and runtime artifact.

## Interpretation

The census finds a much sharper cliff hierarchy than the earlier single selected tuple exposed:

```text
16 residual port tuples
  -> 12 contradictions at root
  -> 4 root-live tuples / 216 root-live boundary pairs
  -> 2 tuples proven infeasible during bounded search
  -> 2 unresolved tuples / 108 unresolved boundary pairs
```

This is exact only inside the accepted fixed predecessor subspace. It is not a global rule that
Phase 3 has only two legal port tuples, and `Unknown` is not feasibility evidence.

The two non-root proofs are independently reproduced in both the embedded residual-tuple parent
and this unrestricted legal-boundary census. They may therefore be excluded exactly from a future
unresolved-search cohort without another proof wave. They remain in `U_root` when measuring whether
the nine-literal guarded clause adds root propagation.

## Next Exact Experiment

Run the predeclared root-only guarded-clause breadth A/B over all 216 members of `U_root`:

1. generate exactly 216 distinct `(tuple, key)` cases from the serialized census;
2. complete a fresh 216-case control root wave;
3. only then complete a fresh 216-case replay root wave;
4. prohibit solver reuse, feedback, wave overlap, or branch search;
5. require each pair to differ by exactly one nine-literal guarded clause, nine terms, nine
   incidences, and zero variables;
6. report all nine literal root states, clause state, projection match, control liveness, replay
   root conflict, and exact domain deltas;
7. aggregate the already-proven-dead tuples 3 and 9 separately from unresolved tuples 6 and 12.

Only tuple 6 matches both retained residual-port atoms in the current guarded core. Its 54 children
are therefore the positive high-level projection stratum; the other 162 are exact negative
controls. Effects limited to tuples 3 and 9 do not resolve the live cliff. Effects in tuple 6 or
other exact live-domain changes justify a small cumulative cut-generation experiment.

Use the 108 children of tuples 6 and 12 only for a separate counterbalanced five-second
search-performance A/B. Do not mix proven-dead and unresolved cohorts in one performance mean.

## Same-Layer Crossing Hypothesis

The user proposed a later progressive-relaxation experiment:

```text
forbid same-layer belt-belt and pipe-pipe crossings
  -> seek a restricted legal witness
  -> remove the restriction completely
  -> pass the witness only as a non-binding hint to the original exact joint model
```

The current model scale makes this hypothesis plausible but not yet proven. It contains 512 bridge
variables, 2,048 bridge-rotation variables, 22,216 aggregate crossing constraints, and 23,752
recorded `bridge-crossing` constraints. A crossing-free restriction is not semantics-preserving by
itself because it excludes legal bridge layouts and may exclude the optimum. It is valid only as an
explicit auxiliary witness generator whose failure proves nothing and whose output never restricts
the fully relaxed final model. This experiment must not replace the exact 216-case breadth A/B.

## Artifacts And Verification

Runtime artifacts:

```text
/tmp/aic-guarded-core-boundary-census.xkoP3O/summary.json
/tmp/aic-guarded-core-boundary-census.xkoP3O/stdout.json
/tmp/aic-guarded-core-boundary-census.xkoP3O/summary.html
/tmp/aic-guarded-core-boundary-census.xkoP3O/census-case-00.observation.html
...
/tmp/aic-guarded-core-boundary-census.xkoP3O/census-case-15.observation.html
```

All sixteen HTML case artifacts exist. `summary.json` and `stdout.json` are byte-identical except
for the stdout trailing newline. The complete artifact directory is 368 MiB; `summary.json` is
159 MiB and `summary.html` is 49 MiB because the full predecessor chain is nested. The next
432-build breadth report must use a compact per-case DTO and one shared predecessor certificate
instead of copying the parent and layouts into every child.

Verification completed before this report:

```text
cargo fmt --all -- --check
cargo check --workspace
cargo test -p aic-data guarded_core --lib
cargo test -p aic-cli --bin aic-prior-terminal-pair
cargo build --release -p aic-cli --bin aic-prior-terminal-pair
git diff --check
```

The release invocation uses the exact parent-chain parameters from the preceding guarded-core
replay report and adds:

```text
--census-guarded-core-boundary-keys
--guarded-core-boundary-census-time-limit-ms 5000
--output-dir /tmp/aic-guarded-core-boundary-census.xkoP3O
```
