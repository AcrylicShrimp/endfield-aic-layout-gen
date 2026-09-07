# Compound-action experiment

Status: measured; no model training in this experiment; overall goal remains active.

The schema-1 interaction harness tested three development factories across eleven levels.
All 66,949 declared candidates were visited (61,897 on strong Heavy Xiranite), with no
routing-budget unknowns and 2,267 candidate-phase validations. Every actual intermediate
and compacted candidate sent to validation passed. Summed level runtime was 17.0 seconds.
Full hashes, actions, phase scores, retained witnesses and work accounting are in
`target/layout-interaction-02/report.json` and its per-level directories. Run 01 produced the
same scores/counts before validation-failure handling was hardened; run 02 is authoritative.

Compaction alone changed none of the three roots. On Iron Nine, a dependency-contact move
of facility `S` by `(-3,+5)` changes score `(364,35,9)` to `(364,55,13)`: a valid deterioration.
Subsequent contraction removes rows 7–10 and reaches `(308,47,13)`, improving the highest-
priority area objective. Void-aligned movement plus compaction separately reaches area 322.
Contact moves without compaction and compaction alone both retain area 364. This is a
concrete delayed-benefit witness, not a claim that a trained policy has learned it.

On strong Heavy Xiranite, none of the new compound or void/contact levels improves area
2,132. Rotation/port changes retain the prior secondary-objective result of 388 transport
tiles. Selected fitting voids and dependency contacts are finite heuristic domains, not all
placements; local external anchors and other current-kernel restrictions remain. No global
impossibility is established, and the 1% density requirement is still unmet.

The user noted that this already-good factory may need a well-trained policy and long
action sequences, then explicitly asked whether these tests involved training. They did
not. Single-step/compound enumeration is now concluded for this phase. Next work must
actually train the expanded-action sequential policy, allow valid temporary deterioration
while preserving the best incumbent, and evaluate long trajectories against untrained
controls. Broader action freedom can be revisited when trajectories expose a concrete
restriction; an ever-growing enumeration matrix must not substitute for learned results.

Eight tests cover deterministic XY domains, compaction control, the exact delayed-benefit
example, invalid intermediate rejection, retained intermediate improvements when the final
phase fails, late results, work limits and fatal validator faults. Negative tests exposed
the chain coverage validator's throwing convention; the new harness now converts only
its known `chain-factory-coverage-invalid` result into candidate rejection. Unknown errors
still abort. A frozen external JSON fixture preserves the positive example for regression.

```sh
node --test docs/benchmarks/layout-production/*.test.mjs
node docs/benchmarks/layout-production/interaction.mjs --plan docs/benchmarks/layout-production/interaction.json --output-directory target/layout-interaction-02
cargo check --workspace
cargo test --workspace
```

Use a fresh output directory for reproduction. Compared with the accepted constructive
planner design: complete validated factories and original production obligations are
preserved, intermediate/compacted work is charged, lexicographic score priority is unchanged,
and no global exact solver, production canvas limit or facility-only fallback was introduced.
