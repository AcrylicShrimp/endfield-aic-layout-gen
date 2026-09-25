# Root Gumbel rank-coupling probe

Status: bounded diagnostic complete. The full on-policy learner experiment is
still running; this result does not establish an improved search setup.

## Question and method

Can changing the outer policy reorder the same physical root actions so that
the current deterministic Gumbel draw is reassigned to different actions?
Use the paired `s1-g1` on-policy collection reports from `current` and
`success-on`: same 32 problem graphs, case IDs, root seed, search config and
initial root states, but different learned outer models. Compare action
identities, their array ranks, actual visited root candidates and first
selected actions. Reconstruct the current Gumbel stream and initial 16
candidates from the committed search configuration, then counterfactually
assign deterministic Gumbels using each action's serialized identity. The
counterfactual computes initial candidate membership only; it does not rerun
MCTS or predict completion.

Both models have the same first-root action set for every case, averaging
2,684.5 actions. Only 0.23% of actions retain their array position; the mean
Spearman rank correlation is 0.74 and the mean median absolute rank shift is
325 positions. Their first selected action matches in 0/32 cases.

The probe reproduces the actually visited 16-action candidate set for all
64 model-case roots. Under the current rank-index Gumbel assignment, paired
models share exactly 8/16 candidates on every case: the eight guaranteed
anchors. With the same Gumbel marginal assigned by stable action identity,
the counterfactual initial sets share 15.81/16 candidates on average (range
15–16). Thus the current assignment nearly replaces the entire stochastic
half of the candidate set when the policy reorders actions.

A second probe on the separate 64-case on-policy diagnostic panel, comparing
the two models after their first generation, reproduces all 128 actually
visited candidate sets. The current rank-index assignment shares 8.02/16
candidates on average; the action-keyed counterfactual shares 15.97/16.
The first selected action again matches in 0/64 cases. This corroborates the
candidate-remapping mechanism on a larger, disjoint panel, while still not
testing completed-layout performance under the counterfactual algorithm.

## Interpretation and limit

In `hierarchical_search.rs`, edges are sorted by learned prior before the
Gumbel vector is drawn from the root seed, case ID and state identity. The
vector is indexed by the sorted array. The rank coupling directly explains
why a model update can receive different random perturbations and stochastic
root candidates on the same problem, even with an unchanged search seed.
It can amplify measured volatility and weaken paired comparisons. The outer
priors and later search Q also change across models, so this probe does not
attribute the observed completion loss to Gumbel remapping alone. A full
paired search with action-keyed Gumbels and runtime measurement is required
before changing the algorithm.

## Reproduction and provenance

- Probe source commit `f240533b`,
  `tools/factory-edit-pilot/hierarchical_gumbel_rank_probe.py`.
- From `~/Srv/aic-target-causal-20260925`, run
  `python3 tools/factory-edit-pilot/hierarchical_gumbel_rank_probe.py --current artifacts/recovery-success-onpolicy/collections/s1-g1-current-source.json --candidate artifacts/recovery-success-onpolicy/collections/s1-g1-success-on-source.json --search-config docs/benchmarks/hierarchical-target-causal/c16-n128-search.json --output artifacts/recovery-success-onpolicy/diagnostics/s1-g1-rank-probe.json`.
- The compact [case-level result](2026-09-25-hierarchical-onpolicy-rank-probe.json)
  records both source report hashes, search config hash, all 32 per-case
  measurements and the 64/64 reconstruction check. The
  [diagnostic-panel result](2026-09-25-hierarchical-onpolicy-rank-probe-diagnostic.json)
  retains the same measurements and 128/128 reconstruction for 64 fresh
  problems. For the second probe, use the same command with `--current`
  `artifacts/recovery-success-onpolicy/evaluations/s1-g0-current.json`,
  `--candidate` `artifacts/recovery-success-onpolicy/evaluations/s1-g0-success-on.json`,
  and `--output` `artifacts/recovery-success-onpolicy/diagnostics/s1-g0-diagnostic-rank-probe.json`.
  The raw reports are part of the still-active on-policy experiment and will
  be closed with it.
