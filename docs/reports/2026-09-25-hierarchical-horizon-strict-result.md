# Declared-namespace horizon confirmation and case-ID sensitivity

Status: closed. Horizon 12 passes the predeclared frozen-search gate under the
declared namespace. This does not establish learner retention.

## Question and controlled method

The preceding confirmation accidentally used namespace
`hierarchical-target-learner-confirmation-v1` instead of the declared
`hierarchical-recovery-horizon-confirm-v1`. Does that change the measured
performance or the horizon-12 conclusion?

Generate 512 candidate training cases with the original seed `20262560`, use
the declared namespace, exclude the same 48 screen problem identities, and
take the first 64. Compare against the earlier panel before search: all 64
problem graphs match in the same order; all 64 case IDs differ. Thus this is
a paired case-ID/noise intervention, not a new graph-difficulty sample.
Keep the generation-87 checkpoints, search configurations, 128 simulations,
and root seeds 20260922/23/24 fixed. Run horizon 6 and 12 for each root seed.

| Namespace and horizon | Completed layouts, three seeds | Robust successes | Total wall time |
| --- | --- | ---: | ---: |
| Earlier ID, horizon 6 | 25 / 21 / 32 of 64 | 25 | 604.1 s |
| Earlier ID, horizon 12 | 35 / 37 / 38 of 64 | 37 | 638.6 s |
| Declared ID, horizon 6 | 22 / 24 / 19 of 64 | 18 | 606.4 s |
| Declared ID, horizon 12 | 34 / 38 / 33 of 64 | 36 | 636.7 s |

Under the declared ID, horizon 12 gains 12, 14, and 14 completions per seed
(paired gain/loss counts 18/6, 17/3, 17/3). Two of 18 robust baseline
successes are lost. All three seeds improve, mean gain is 13.33/64, and the
predeclared gate of mean gain at least four, two winning seeds, and at most
three lost robust successes passes. First actions match the paired horizon-6
search in 39, 44, and 41 cases. Runtime rises by 5.0%.

## Interpretation

The namespace does affect measured performance. The generator uses the
numeric seed for graph draws and puts namespace only in the case ID. The
search hashes that ID, root seed, and state identity to draw Gumbel noise.
Changing the namespace therefore changes search decisions on identical
problems. Across namespaces, the horizon-6 success masks gained/lost
12/15, 12/9, and 8/21 cases; horizon 12 gained/lost 14/15, 11/10, and 12/17.
The largest total change is horizon 6 on the third seed, 32 to 19 successes.
This is substantial case-ID sensitivity. A single seed and ID namespace are
insufficient for a stable absolute success estimate.

The horizon-12 direction remains positive under both ID assignments and all
three root seeds. Its improvement is therefore more credible than either
namespace's raw success count. The next research question is whether training
on improved targets retains those benefits across generations; this frozen
search result alone cannot answer it. A future evaluation should use several
explicit search seeds and stable graph-based case identity when making
cross-panel comparisons.

## Reproduction and provenance

- Source commit `d0e63510`; runner:
  `tools/factory-edit-pilot/hierarchical_horizon_confirmation.py`.
- Remote workspace `~/Srv/aic-target-causal-20260925`; output
  `artifacts/recovery-horizon-strict`.
- From that workspace with the documented LibTorch environment:
  `python3 tools/factory-edit-pilot/hierarchical_horizon_confirmation.py prepare --output artifacts/recovery-horizon-strict --experiment-commit d0e63510 --reference-panel artifacts/recovery-horizon-confirm/panels/confirmation.json`,
  then `run` and `summarize` with the same `--output`.
- [Protocol](2026-09-25-hierarchical-horizon-strict-protocol.json) holds
  exact binary, sampling, panel, checkpoint, and search configuration hashes,
  plus the 64-graph equality check. [Progress](2026-09-25-hierarchical-horizon-strict-progress.json)
  holds raw report hashes and runtimes. [Summary](2026-09-25-hierarchical-horizon-strict-summary.json)
  retains paired case outcomes and the fixed gate. The
  [earlier confirmation](2026-09-25-hierarchical-horizon-confirmation-result.md)
  records the other namespace.
