# Gumbel candidate churn across training generations

Status: bounded diagnostic complete. The on-policy learner experiment remains
active; no search algorithm change is inferred from this diagnostic alone.

## Question and method

Does rank-index Gumbel assignment replace stochastic candidates after ordinary
training updates, even within one treatment? On the same 64-case diagnostic
panel, compare generation 1 to 2 and 2 to 3 separately for `current` and
`success-on` in stream `s1`. The starting model and all these evaluations use
the same search seed, case IDs and search config. Run the existing root rank
probe on each consecutive checkpoint pair. It verifies that physical first
root action sets match, reproduces the 16 actually visited candidates, and
counterfactually assigns Gumbels by physical action identity without running
another MCTS search.

| Treatment, generations | Mean rank correlation | Exact positions | Same first actions | Actual common candidates / 16 | Action-key counterfactual / 16 |
| --- | ---: | ---: | ---: | ---: | ---: |
| Current, 1→2 | 0.979 | 0.97% | 0/64 | 8.14 | 16.00 |
| Current, 2→3 | 0.955 | 0.91% | 1/64 | 8.08 | 16.00 |
| Success-on, 1→2 | 0.744 | 0.21% | 0/64 | 8.02 | 15.88 |
| Success-on, 2→3 | 0.594 | 0.13% | 0/64 | 8.00 | 15.55 |

All 512 actual root candidate sets are reconstructed exactly. The eight
guaranteed anchors account for nearly all common candidates under the
current assignment. Even the `current` model's relatively similar ranks
(correlation 0.979 between the first two generations) move enough action
indices to replace almost all eight stochastic candidates. Under the
counterfactual action-key assignment, the candidate sets remain almost
identical across generations.

## Interpretation and next check

The per-rank Gumbel stream adds a discontinuity to the training/evaluation
loop: a modest prior-rank change reassigns random draws to physical actions.
This is direct evidence for candidate churn and a plausible contributor to
observed completion volatility. It does not isolate how much completion
volatility is caused by that churn, since the learned prior and search Q also
change. A bounded release-mode search comparison must test actual completion,
retention and runtime before adopting a stable Gumbel key. The action batch is
generated before the outer-prior sort, so an original-generation ordinal is
a possible lower-cost key while lower guidance remains frozen; it must be
verified across models and placement-page widening.

## Reproduction and provenance

- Probe code commit `f240533b`,
  `tools/factory-edit-pilot/hierarchical_gumbel_rank_probe.py`.
- Remote workspace `~/Srv/aic-target-causal-20260925`; input reports under
  `artifacts/recovery-success-onpolicy/evaluations/` and config
  `docs/benchmarks/hierarchical-target-causal/c16-n128-search.json`.
- For each arm `current` and `success-on`, run the probe with `--current`
  `artifacts/recovery-success-onpolicy/evaluations/s1-g0-ARM.json` and
  `--candidate` `.../s1-g1-ARM.json`, then repeat for `g1` to `g2`. Use the
  config above and a separate `--output` under the experiment's
  `diagnostics/` directory for each pair.
- The [compact result](2026-09-26-hierarchical-onpolicy-generation-rank-probe.json)
  records all four source-report hashes, full probe hashes, search config
  hashes, case-level candidate overlaps, and aggregate results. The raw
  reports remain part of the still-active on-policy experiment.
