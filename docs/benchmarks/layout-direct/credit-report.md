# Quality-first temporal credit result

Status: measured; reject a large training-budget increase on this evidence alone.

## Outcome

The second main-path run completes 64 updates and 131,072 actually executed training actions
(four 512-step trajectories per group), followed by 39 full 1,024-step control evaluations.
All 295 retained episode outputs pass fresh geometry and original factory-coverage audits.
Wall time is 800.29 seconds, including generation, training, evaluation and brief overlapping
local browser/test diagnostics. It is not an isolated performance benchmark.

Quality-first futility tie-breaking supplies terminal comparison signal to all 64 groups;
58 have quality-only signal, and 59,517 actions have nonzero prefix-baseline credit. All eight
Heavy Xiranite groups now receive a task gradient, including the six whose quality tuples tie.
29,043 of 29,537 parameters change; checkpoint reload preserves the exact direct action and log
probability. This fixes absent credit in the observed ties, not the learned-quality problem.

| Development policy | Mean retained / initial area | Futile proposals | Trained wins / ties / losses |
| --- | --- | --- | --- |
| Initial | 0.190849 | 81.70% | 5 / 2 / 5 |
| Uniform conditional | 0.192249 | 81.88% | 6 / 1 / 5 |
| Trained, temporal credit | 0.186203 | 82.07% | — |

The mean area improves slightly but exact-score wins and losses remain mixed; development waste
gets worse. Heavy Xiranite stays `[2132, 392, 147]`, 29.6904% empty, for all three controls. Its
trained futile rate is 83.59%, versus 84.18% initial and 84.77% uniform: far from the release gate.
Two training trajectories reduce target wiring by one tile, without area improvement. The final
policy does not reproduce that reduction in the evaluated target episode.

The target's 1,024 trained actions remain close to uniform across operations: 195 relocations,
210 poses, 215 port changes, 206 reroutes and 198 compactions. Only two relocations, two poses,
162 port changes and two reroutes reach novel valid states; no action improves the retained
target. 207 poses are rejected, 204 reroutes are unchanged, and every compaction is unchanged.
Therefore even the small target-waste reduction does not demonstrate useful packing behavior.

## Fairness and scope

The same seed regenerates the original stream from catalogs, not by loading saved training roots.
The 56 eligible training and 12 eligible validation roots have identical production and complete
root hashes to the first run; initial model checkpoint bytes also match. This is deliberate
cross-configuration replay, not 56 additional distinct production problems. Constructor failure
counts and accepted-size distributions match: the largest new training factory still has ten
facilities. Heavy Xiranite remains explicitly trained replay, not held-out generalization.

The experiment changes credit, training horizon and learning rate together, as preregistered in
`credit-contract.md`. It cannot isolate their individual causal effects. It has one model seed
and one evaluation seed, so no effectiveness claim or population confidence interval is made.
The modest mean-area change is insufficient reason to launch larger or repeated training on
the same restricted exposure while required medium/large strata remain missing.

Decoding and masking time are now retained per evaluation worker: development means are 1.111s
initial, 0.321s uniform and 1.100s trained per 1,024-proposal episode. They are also included in
co-batched wall times and must not be added twice. Environment preprocessing is separate. Routing
work caps match, but actual consumed expansions do not; no matched-consumed-work advantage is
claimed. Every proposal still uses original pipe-first repair and independent validation.

## Decision

Keep the new accounting and explicit credit contract as measured main-path infrastructure;
do not claim the learned model is ready, and do not expand this weak setting into a larger
seed/budget sweep yet. Next implement the separate broader target-load contract so medium/large
factories can enter actual streaming training. Report solved size, constructor failure and
observation-support attrition before claiming coverage. The current-extent and fixed-external-
anchor action limits are still explicit executor limitations to address if main-path evidence
continues to show low useful movement. They are not global infeasibility or game limits.

The first run's same-scale geometry audit is in `visual-report.md`. It demonstrates loose
packing even in an area-winning example and a real small-factory regression. No new visual
readiness claim follows from this second run's metrics or unchanged target witness.

## Verification

```sh
uv run --frozen --project docs/benchmarks/layout-learning python docs/benchmarks/layout-direct/train.py \
  --plan docs/benchmarks/layout-direct/sampling.json --config docs/benchmarks/layout-direct/credit.json \
  --output-directory target/layout-direct-credit-01
node docs/benchmarks/layout-direct/analyze.mjs \
  --report target/layout-direct-credit-01/report.json --output docs/benchmarks/layout-direct/credit-results.json
uv run --frozen --project docs/benchmarks/layout-learning python -m unittest discover \
  -s docs/benchmarks/layout-direct -p 'test_*.py'
node --test docs/benchmarks/layout-direct/*.test.mjs
cargo check --workspace
cargo test --workspace
```

Use fresh output paths. Nine Python tests and ten direct Node tests pass. New checks cover
quality-priority tie-breaking, pre-action-only temporal credit, rejected/unchanged accounting,
unperformed late proposals and explicit truncated-training failure. Runtime sources remain
unchanged throughout the run and pass shutdown provenance checks. Workspace check/tests pass;
there are no Rust edits. The architecture still matches the accepted constructive design:
complete connected construction, private validated exploration, unchanged best-score priority,
pipe-first repair and no global exact fallback. No production API or CLI cutover is claimed.
