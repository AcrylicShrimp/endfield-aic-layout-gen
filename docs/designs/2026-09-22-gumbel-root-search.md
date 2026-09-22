# Gumbel root search for hierarchical layout construction

Status: accepted implementation and evaluation contract

## Problem and scope

The generation-nine outer policy completes only 9/32 cases on the fixed
policy-collapse panel at 64 simulations. Its first-action distribution is
concentrated, and prior-ranked progressive widening prevents most anchor-ranked
recovery actions from receiving a visit. Increasing the budget to 1,024 raises
completion to 19/32 but costs about five times the wall time of the 64-simulation
run; 2,048 simulations adds no completions. The fixed anchor-prior control
completes 30/32 cases. Search therefore needs bounded exploration that cannot be
censored by the learned ranking.

This change applies the root treatment from Gumbel AlphaZero to every real
receding-horizon decision. It does not replace the lower route generator or the
PUCT traversal below the root. Keeping the internal tree unchanged isolates the
effect of root allocation and preserves the existing native validation,
candidate batching, compiled inference and resource bounds.

## Configuration contract

Search configuration schema `aic-hierarchical-search-config-v2` has an explicit
tagged `root_selection` field:

```json
{
  "algorithm": "gumbel-sequential-halving",
  "candidates": 16,
  "anchor_candidates": 8,
  "anchor_policy_mass": 0.5,
  "seed": 20260922,
  "value_scale": 0.1,
  "max_visit_init": 50.0
}
```

`puct` retains the previous root behavior for controlled comparisons. Gumbel
candidate count is bounded, the anchor quota cannot exceed it, and the
simulation budget must cover at least one visit in every sequential-halving
round. The seed is combined with the case identity and exact root-state digest,
so reruns are deterministic while distinct generation cases and states receive
different perturbations.

## Root algorithm

At a Gumbel root, placement pages are exhausted before scoring. Route and
network candidates are generated once, all legal root macros are natively
applied to clones once, and the outer model scores the single complete batch.
This makes the candidate pool independent of a collapsed learned ranking and
avoids repeated full-batch inference as progressive widening adds pages.

For each action, retain the mixed search prior and the independent anchor prior.
Choose a fixed-size considered set by:

1. taking the highest-anchor actions up to `anchor_candidates`;
2. filling the remaining slots by Gumbel Top-k of `log(mixed_prior) + g`; and
3. using stable action indices to break exact ties.

The anchor quota guarantees evaluation, not selection. This prevents the
collapsed model from excluding the deterministic search's best alternatives
while still allowing observed continuation value to choose the action.

The root policy is a conservative mixture of the model's normal search prior
and the independent anchor prior. `anchor_policy_mass` applies only to Gumbel
candidate sampling, sequential-halving ranking and the resulting improved
policy target. This is stronger than merely inserting anchor actions: the
collapsed model cannot immediately suppress those actions again through its log
prior. Internal PUCT retains the normal mixed prior.

Sequential halving divides the configured simulations among the current
survivors. Each simulation is forced through its assigned root edge and then
uses the existing PUCT selection, progressive widening, learned leaf value and
exact terminal value below that edge. After a round, retain the best half by:

```text
gumbel + log(mixed_prior) + completed-Q transform
```

Unvisited actions receive the root's visit-weighted mixed value. Completed Q is
min-max normalized over the root and scaled by
`(max_visit_init + max_child_visits) * value_scale`, matching the bounded
monotonic transform used by the reference Gumbel search. Any budget left after
one candidate remains is spent on that candidate. The final survivor is the
committed action.

## Training target

Sequential-halving visit counts encode the allocator, so they are not a valid
policy target. Gumbel root records therefore store the improved policy

```text
softmax(log(mixed_prior) + completed-Q transform)
```

over the complete root action set. Generation consumes that distribution
directly. PUCT records retain the existing temperature-adjusted visit target.
The report also records anchor priors so candidate coverage and future policy
collapse can be diagnosed without rerunning inference.

## Controlled evaluation

Use the same fixed 32-case panel, generation-nine lower and outer checkpoints,
catalogs and machine as the budget diagnosis. Compare Gumbel budgets 64, 128
and 256 against the retained PUCT results at 64 and 1,024 simulations. Start
with 16 considered actions and eight guaranteed anchor actions. If the official
Q-transform scale cannot retain useful anchor actions, adjust only the bounded
Gumbel controls and record the treatment.

Adopt the smallest Gumbel budget that reaches at least 27/32 completions, or is
within one case of the best Gumbel result when none reaches that threshold. It
must improve materially over PUCT-64 and use less wall time than PUCT-1,024.
If no treatment does so, retain the implementation as an explicit research
mode and do not switch the curriculum configuration.

## Verification and continuity

- Native macro application and independent final validation remain unchanged.
- Lower routing candidates, compiled inference, CPU limits and the fixed search
  horizon remain unchanged.
- PUCT remains selectable for comparison and old behavior is covered by an
  explicit configuration rather than an implicit default.
- Unit checks cover deterministic Gumbel sampling, anchor inclusion, the
  minimum sequential-halving budget and normalized improved-policy targets.
- The integrated CUDA harness provides the adoption evidence; a successful
  local build alone is insufficient.
