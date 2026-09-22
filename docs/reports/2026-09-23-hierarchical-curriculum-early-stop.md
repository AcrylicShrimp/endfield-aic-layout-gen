# Hierarchical curriculum early-stop verdict

## Verdict

Do not resume the corrected 100-generation curriculum. Retain its 88 committed
generations and proceed with a stability-first investigation. Level 1 ran for
84 generations without a useful upward trend, fixed held-out performance
oscillated between 32 and 47 completions out of 128, and the final evaluation
completed 38. Another 12 generations cannot justify an ultra-long run from this
trajectory.

The next production inference path must use eager execution only. Generation 86
first stopped on a nondeterministic TorchScript route-guidance parity failure;
the same binary passed after restart and reached 88 committed generations. The
runtime compiler path therefore adds a long-run failure mode for a retained
throughput benefit of only about 7.64% in the
[retained hierarchical-routing benchmark](../designs/2026-09-22.00-solver-augmented-hierarchical-routing.md).
Recover material inference cost later through batching. The eager cutover is tracked in
[`2026-09-23.01.cut-hierarchical-production-inference-to-eager-only.md`](../todos/2026-09-23.01.cut-hierarchical-production-inference-to-eager-only.md).

Machine-readable values and identities are in
[`2026-09-23-hierarchical-curriculum-early-stop-evidence.json`](2026-09-23-hierarchical-curriculum-early-stop-evidence.json).

## Retained run

The run promoted from level 0 after generation 3, when its 128-case recent
training window reached 89.84%. Generations 4 through 87 remained at level 1.
The server state records 88 committed generations and level 1. Generation 88
contains only a sampled `training-panel.json`; it has no collection, update,
summary, or committed checkpoint and is excluded from every result below.

The closeout created an immutable `stop.json` beside the server output. Its
SHA-256 is
`231146bfa197da7479caecc84b4fb0d1be41dc41ccd46705708e98af292b19c9`.
The retained binary SHA-256 is
`6b0c7fad7869f63da3859c35fa077500887a2fd1835067fbcc8871823bb5c747`;
the final lower and outer checkpoint hashes are recorded in the evidence file.
The launch manifest's `source_commit` value is malformed, so it is not used as
an identity. The binding, binary, configuration, input, checkpoint and retained
source-file hashes remain available.

The independent observer exported generations 0 through 87. The
[W&B run](https://wandb.ai/acrylicshrimp-personal/endfield-aic-layout-gen/runs/7718b4f7291e77b636a0702b)
was closed with state `finished`, history step 87, source status
`stopped-early-instability`, the stop reason and final metrics. No trainer or
observer process remains active.

## Learning trajectory

Level-1 training completion averaged 52.75%, ranged from 28.12% to 78.12%, and
had a linear slope of only +0.0178 percentage points per generation. Its first
ten generations averaged 56.56%; its last ten averaged 52.50%. The final
128-case recent window was 49.22%. The run never approached the 75% promotion
threshold sustainably.

Fixed held-out results show the same absence of progress:

| Generation | 4 | 9 | 14 | 19 | 24 | 29 | 34 | 39 | 44 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| Completed / 128 | 37 | 45 | 44 | 39 | 46 | 40 | 38 | 32 | 42 |

| Generation | 49 | 54 | 59 | 64 | 69 | 74 | 79 | 84 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Completed / 128 | 47 | 40 | 41 | 37 | 36 | 45 | 42 | 38 |

The mean was 40.53 and the fitted slope was -0.0176 completed cases per
generation. The final facility strata were 11/12 for one facility, 26/52 for
two facilities and 1/64 for three facilities. By connection count, the final
results fell from 6/6 at two connections to 1/21 at six and 0/34 across seven
and eight connections. All 90 final held-out failures terminated with
`no-macro-actions`.

This is not a recurrence of the earlier initial-root prior collapse. Across
generations 4, 9, 49, 84 and 87, mean normalized initial-root entropy stayed
between 0.999897 and 0.999993. Mean maximum prior stayed near 0.00035 to
0.00040. The behavioral result still oscillated and forgot difficult cases.
The evidence therefore points to a learning-retention or target/credit problem,
not a concentrated initial proposal distribution.

Optimizer activity does not establish stability. Outer loss improved within 55
of 88 generation updates and lower loss improved within 73. Maximum gradient
norm had a median of 146.37, reached 1,203.46 and was 557.91 in generation 87.
These values show that training continued to make large updates, but they do
not isolate the cause of the held-out regressions.

## Runtime interruption

The first generation-86 resume process stopped with diagnostic
`aic-learning.route-guidance-compiled-parity`: the traced result differed from
the eager result by a maximum absolute value of `1.227617e-3`. An immediate
retry with the same binary and durable state passed the first-shape trace and
committed generations 86 and 87. That process was then terminated deliberately
before generation 88 collection.

The successful retry does not make runtime tracing reliable. It demonstrates
that the guard is process-dependent while all durable model and input
identities remain unchanged. Production runs should not trace new shapes or
execute eager/compiled parity checks in their hot path.

## Next gate

The existing post-run tracker now owns the stability investigation:
[`2026-09-23.00.evaluate-completed-curriculum-and-choose-ultra-long-training-path.md`](../todos/2026-09-23.00.evaluate-completed-curriculum-and-choose-ultra-long-training-path.md).
It must identify the smallest correction for retention, target or credit
instability and pass a bounded fixed-held-out comparison before another long
run. The eager-only cutover can proceed independently. An ultra-long curriculum
remains blocked until both gates pass; throughput work should then start with
batched inference and an integrated measurement.
