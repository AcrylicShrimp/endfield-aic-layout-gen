# Adapted Gumbel root search

## Verdict

Adopt the problem-adapted Gumbel root allocator with 128 simulations per
committed macro action. On the fixed 32-case policy-collapse panel it completes
27 cases in 39.71 seconds. The former PUCT search completes 9 cases at 64
simulations and 19 cases at 1,024 simulations; the latter takes 67.56 seconds.
Gumbel-128 therefore reaches the predeclared 27-case threshold, recovers eight
more cases than PUCT-1,024, and uses 41% less wall time.

Gumbel-256 completes 28 cases in 58.80 seconds. The extra 19.10 seconds buys one
additional completion, so 128 is the selected budget. Full machine-readable
results and identities are in
[`2026-09-22-adapted-gumbel-root-evidence.json`](2026-09-22-adapted-gumbel-root-evidence.json).
After the comparison, a configuration-boundary fix made the minimum budget
check exact for non-power-of-two candidate counts. It does not change the
selected 16-candidate path. A fresh release build of that final source repeated
the selected cell at 27/32 in 38.33 seconds.

## Why the reference algorithm needed adaptation

The layout search differs from the board-game setting assumed by Gumbel
AlphaZero. Its actions are variable-size, natively validated placement, route
and network macros. Placement actions arrive through domain-ordered pages, and
the learned policy being searched may itself be collapsed. A visit allocator
that assumes its input policy remains a useful proposal can therefore include
an anchor action and immediately suppress it again through the learned log
prior.

The adopted root treatment changes four boundaries:

- Every real receding-horizon root materializes and scores the complete current
  macro-action set once. The tree below the root retains the existing PUCT,
  progressive widening, compiled inference and lower routing implementation.
- Sixteen root candidates are considered. Eight are guaranteed by the
  independent anchor ranking and eight are filled by Gumbel Top-k.
- Candidate sampling, sequential-halving ranks and the improved policy target
  use a root-only 50/50 mixture of the normal search prior and the independent
  anchor prior. Internal PUCT continues to use the normal prior.
- Training consumes the completed-Q improved policy. It does not imitate the
  sequential-halving visit counts, which describe the allocator rather than a
  policy preference.

The Gumbel seed is combined with the case identity and exact root-state digest.
Runs are reproducible, while different generated cases and later macro states
receive different perturbations. Every chosen macro still passes native apply
and final independent validation.

## Controlled results

All rows use the same generation-nine lower and outer checkpoints, normalized
catalogs, fixed case order, search horizon and 32-case training-only diagnostic
panel. No model was trained during this comparison.

| Root treatment | Simulations | Completed | Wall seconds |
|---|---:|---:|---:|
| PUCT | 64 | 9/32 | 13.24 |
| PUCT | 1,024 | 19/32 | 67.56 |
| Unmixed Gumbel | 64 | 19/32 | 23.36 |
| Unmixed Gumbel | 128 | 23/32 | 37.02 |
| Unmixed Gumbel | 256 | 25/32 | 55.53 |
| Conservative-prior Gumbel | 64 | 25/32 | 24.98 |
| **Conservative-prior Gumbel** | **128** | **27/32** | **39.71** |
| Conservative-prior Gumbel | 256 | 28/32 | 58.80 |

The selected 128-simulation row completes all 8 easy controls, 15 of 17 early
hard successes and 4 of 7 early hard failures. PUCT-64 completes 6, 3 and 0 in
the same groups. Every initial Gumbel root evaluated exactly 16 candidates and
all eight guaranteed anchor candidates.

Two plausible adaptations were rejected. Preserving separate anchor and
learned survivor lanes completed 17, 20 and 23 cases at 64, 128 and 256
simulations: reserving half the later-round work for the anchor lane reduced
useful evaluation. Restricting the proposal pool to eight placement pages
completed 18, 18 and 19 cases and made the 256-simulation cell slower. The
successful change was to preserve the ordinary sequential-halving allocator
while correcting the proposal prior used at the root.

## Code and data contract

Search configuration is now schema `aic-hierarchical-search-config-v2` and
requires an explicit `root_selection`. Historical benchmark configurations use
explicit `puct`; the curriculum search and matching outer warm-start contract
use the selected Gumbel-128 controls. Search reports are schema
`aic-hierarchical-search-report-v7` and record both normal and anchor priors plus
an optional completed-Q policy target for each action.

Generation uses the stored improved policy whenever it is present and retains
temperature-adjusted visit targets for PUCT records. This is a deliberate schema
cutover: older search configurations and reports fail rather than silently
receiving different policy-target semantics.

The server closeout verified and summarized all 12 comparison reports before
deleting them; it also recorded the smoke-report hash in the retention
manifest. It retained the panel, search configurations, run manifests,
analyses, scripts, final-source verification and top-level release binaries.
Removing raw reports, duplicate checkpoint copies and build caches recovered
16,151,973,888 available filesystem bytes in the first cleanup; the final
verification report was likewise verified and summarized before its raw copy
was removed.

## Limits and next run

This panel is a training-only diagnostic instrument built from earlier
curriculum cases. The result establishes search recovery and selects a bounded
collection budget; it does not establish generalization or long-run curriculum
stability. The next curriculum run should start from the canonical Gumbel-128
configuration, keep the held-out panel untouched, and monitor completion,
policy entropy and the 128-case promotion window before advancing levels.
