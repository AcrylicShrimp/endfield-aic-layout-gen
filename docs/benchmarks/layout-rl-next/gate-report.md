# Breadth and long-horizon pilot gate report

Status: pilot complete, 2026-09-07. Corpus and learning-mechanism gates pass. A better learned
optimizer, beneficial curriculum order, and tightness/optimality are **not established**.
Full multi-seed training, test evaluation, curriculum-order training and residual search have
not started. All pilot subprocesses have finished; there is no unattended training job.

## Completed work

The [design](experiment-plan.md) and [contract](contract.md) separate data breadth, episode
length, curriculum order and long-run inference. The existing constructor, RL environment,
policy architecture, routing engine, reward and validator are unchanged. No production path,
game catalog, prior model or user-owned worktree change was modified.

The generated corpus adds 56 complete factory roots to the prior 23. All 79 passed fresh
Rust geometry/recipe validation and original-request coverage. The 96 new requested variants
produced 56 accepted roots, eight duplicates, 12 constructor stops and 20 unsupported requests.
Unsupported copper chains and oversized battery expansions were not replaced with cropped
factories. Explicit intermediate procurement boundaries belong to the new requests, fixed
before construction; they do not alter any previous request.

| Split | Before | Now | Observed facility counts |
| --- | ---: | ---: | --- |
| Training | 14 | 52 | 2–9, plus one 12-facility root |
| Validation | 3 | 9 | 4–9 |
| Test | 4 | 16 | 4–10 |
| Older out-of-family | 2 | 2 | 59 |

Seven new training product families, one new validation family and two new test families
survive construction. Every new family's depth/pose/rate variants inherit the same split.
The corpus is broader but is not yet a well-covered medium/large curriculum: only one training
root has 12 facilities, and none of the 59-facility factories is trained on. New validation
variants share one product family; nine validation roots are not nine independent tasks.

## Actual pilot

Four CPU runs use 8,192 sampled training actions each (32,768 total), eight environment
workers, width 32, seed 7, and terminal credit except the zero-signal control. Fresh initial
weights isolate the experiment; prior learned models remain available unchanged.

| Arm | Groups | Episodes | Improved training episodes | Informative groups | Distinct sampled roots |
| --- | ---: | ---: | ---: | ---: | ---: |
| H16 terminal | 64 | 512 | 33 | 16 | 31 |
| H16 zero signal | 64 | 512 | 33 | 16 | 31 |
| H32 terminal | 32 | 256 | 28 | 13 | 22 |
| H64 terminal | 16 | 128 | 30 | 7 | 15 |

Every terminal model changed weights and passes exact JSON checkpoint prediction reload;
the zero control changed zero weights. More successful episodes as a *fraction* at H64 is
not evidence of a superior learned policy: episode lengths, update counts and sampled root
coverage differ. Full controls need matched per-root action exposure. Five argmax validation
passes per model add 5,760 actions outside the training allowance; all retain the same initial
validation tuples. This pilot has not made deterministic action selection useful.

Final sampled-policy validation covers nine roots, three proposal caps (128/512/1,024), four
model arms, and retained-initial/uniform/greedy/bounded-best-first controls: 216 rows, rollout
seed 101 only. No test or older out-of-family evaluation was run. The entire pilot orchestration
completed in 97.49 seconds, including validation and audit. Corpus generation took under one
second on this run. This is one workload timing, not a throughput comparison.

All 1,408 saved training best incumbents and all 216 evaluation best incumbents were independently
validated again: **1,624 passes**. Artifact hashes, original semantics, exact score, split use,
strictly improving incumbent histories and equal training action totals also passed.

## Repeated improvement is real; the horizon benefit remains uncertain

At 1,024 proposals every terminal arm improves seven of nine validation roots; six of nine
have improvements after proposal 128. H64's eight-facility iron validation trajectory first
improves after 128 and has its last recorded improvement at proposal 938. Its score at 128
is `[285,37,12]`, at 512 `[252,48,17]`, and at 1,024 `[238,50,19]`. This is a valid late gain,
not proof that H64 training caused it or that the final factory cannot be tightened further.

All terminal arms have four improved roots at the separate 128-proposal allowance and seven
at 512/1,024. The untrained zero-signal model has the same improved-root counts. At 1,024,
H16 and H32 each beat zero on two exact tuples and tie seven; H64 beats zero on one and ties
eight. H16 and H32 beat uniform on six roots and tie three, but much of this advantage is
already present in the untrained architecture. Neither the best tuple nor the improvement
count should be attributed wholly to learning. Full paired outcomes are in
[pilot-results.json](../../../artifacts/benchmarks/layout-rl-next/pilot-results.json), including every validation row and within-rollout curve.

Crucially, H32 ties H16 on all nine exact final scores at 1,024 proposals; H64 ties eight
and loses one on secondary transport/turn costs. All three tie on final area on all nine roots.
Longer exploration finds additional improvements; longer training horizon has not
shown an additional final-quality benefit in this one-seed pilot. Equal-budget short and
long evaluations are separate trajectories whose horizon-context features can differ; the
within-1,024 curves, not cross-run prefix equality, establish late improvements.

## Next experiment decision

Proceed with matched-exposure, multi-seed controls before increasing model size or training
length further. Keep the declared 16/32/64 horizons; construct a common per-root action ledger
and preserve small-root rehearsal. The next curriculum-order contrast should change only
the order/mix of those charged root exposures at H16. Its schedule must be frozen before
test outcomes are inspected. Medium-scale coverage needs improvement before claiming a
small-to-large curriculum; changing the constructor/action domain remains a separate slice.

The eventual tightening contract needs continued improvement from already good incumbents,
separately charged residual search and explicit stop reasons. This work establishes late
improvement measurements, not an operational-tightness certificate. A long no-improvement
tail is only observed stagnation; the current local action/router restrictions remain.

## Verification and artifact retention

- `node --test docs/benchmarks/layout-rl/*.test.mjs docs/benchmarks/layout-rl-next/*.test.mjs`: 16 passed.
- Python unittest discovery: eight existing RL tests and one new pilot-plan test passed.
- `cargo check --workspace` and `cargo test --workspace`: passed; two existing ignored tests remain.
- Both documented corpus/pilot CLIs ran successfully. A repeated corpus output directory was
  rejected with a structured `rl-next-EEXIST` diagnostic and did not overwrite evidence.
- `git diff --check`: passed. No Rust edit required formatting.

Full inputs and source hashes are in `target/layout-rl-next-corpus-01/report.json` and
`target/layout-rl-next-pilot-01/{report,audit}.json`. The external results snapshot pins all
three report hashes and model hashes. Keep these directories for exact audit/replay; they
occupy approximately 13 MiB and 118 MiB respectively. Nothing was deleted.

The pre-existing three modified files and untracked `constructive-recipe-breadth` directory
were preserved. Runtime dependencies on existing working-tree code are content-hashed in
the corpus provenance; a future checkout with different bytes must regenerate the corpus.
