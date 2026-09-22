# Hierarchical curriculum stability diagnosis

## Question and verdict

Why did the corrected hierarchical curriculum fluctuate instead of improving
steadily through 88 committed generations?

**The outer policy is the main observed source of checkpoint-to-checkpoint case
churn.** Its changed action ranking, rather than the lower route model or outer
value output alone, causes many previously solved cases to fail. The training
loop supplies a plausible mechanism: each generation fits 200 epochs to only 32
new cases, with no replay of earlier successes, while failed episodes provide
equally weighted policy targets. Under the 128-simulation, 16-candidate root
search, most target probability remains on unvisited actions. Those ingredients
make policy distillation a weak and moving teacher. This evidence identifies the
responsible component and a concrete instability path; it does not separately
prove which of replay, target qualification or update size is the decisive
correction.

The fixed held-out set was used here to diagnose retained checkpoints. It must
not be used to select or tune the next learner. Machine-readable identities and
per-case results are in
[`2026-09-23-hierarchical-stability-diagnosis-evidence.json`](2026-09-23-hierarchical-stability-diagnosis-evidence.json).
That file also retains each fixed case's 17-evaluation success history as a
little-endian bit mask; bit zero is generation 4 and bit 16 is generation 84.

## The fixed cases change, not merely the total

The 17 retained evaluations used the same 128 cases after generations 4 through
84. Their total completions ranged from 32 to 47; the final result was 38. Yet
adjacent evaluations gained 9-24 cases and lost 10-22 cases. Of the 128 fixed
cases, 13 succeeded in every evaluation, 27 never succeeded, and 88 changed
outcome at least once.

All 52 two-facility cases succeeded at least once, but only three succeeded in
all 17 evaluations. Of 64 three-facility cases, 37 succeeded at least once and
none always succeeded. The run trained at the two-facility curriculum level from
generation 4 onward; the three-facility results measure transfer to a later
level, not mastered training-level performance. The two-facility case churn
shows that an unchanged aggregate can hide forgetting and replacement.

Search is deterministic for a fixed case, configuration and checkpoints: root
Gumbel perturbations derive from the case and root-state identities. The fixed
panel's changing outcomes therefore track changed checkpoints and downstream
states, not freshly sampled evaluation cases.

## Crossed checkpoint comparison

The canonical held-out reports already contain the matched generation-49 and
generation-84 evaluations. Using the retained production binary and same search
configuration, two additional evaluations crossed their lower and outer
checkpoints. All four cells used the same 128-case panel.

| Lower checkpoint | Outer generation 49 | Outer generation 84 |
| --- | ---: | ---: |
| Generation 49 | 47/128 | 40/128 |
| Generation 84 | 43/128 | 38/128 |

Changing only the outer checkpoint loses a net 7 cases with lower 49 fixed and
5 with lower 84 fixed. Changing only the lower checkpoint loses a net 4 and 2,
respectively. Net totals understate the change: with lower 49 fixed, replacing
outer 49 by outer 84 gains 15 cases and loses 22. Among the 52 two-facility
cases it gains 13 and loses 12, even though the stratum total changes only from
26 to 27. The same outer swap reduces three-facility completions from 9 to 2.

This is component attribution for these two checkpoint dates, not a claim that
the lower model never matters or that every intervening generation has the same
decomposition.

## Policy versus value within the outer model

The 22 cases solved by lower-49/outer-49 but failed by lower-49/outer-84 form a
targeted diagnostic panel. Starting from the generation-84 outer checkpoint,
the first ablation zeroed only `outer.model.policy.2.weight` and `.bias`, making
learned action logits uniform while retaining its encoder and value output. The
second zeroed only `outer.model.value.2.weight` and `.bias`, making value outputs
constant while retaining its encoder and policy output. Both kept lower 49 and
the search configuration fixed.

| Outer treatment on the selected 22 cases | Completed |
| --- | ---: |
| Unmodified generation 84 | 0/22 by panel construction |
| Uniform learned policy logits | 12/22 |
| Constant value outputs | 0/22 |

The policy-output intervention recovers 12 actual lost cases; the value-output
intervention recovers none. Since these cases were selected because they
regressed, the 12/22 result is a causal check on the losses, not an unbiased
estimate of whole-panel performance. Uniform logits are not proposed as the
production correction. Shared encoder changes and lower-policy interactions can
still account for remaining failures.

## Why the learner permits oscillation

Generation training minimizes a new objective each time:

```text
parameters[g+1] = update(parameters[g], targets from search(parameters[g], fresh 32 cases))
```

The held-out completion count is not this objective, and there is no constraint
that preserves decisions on prior cases. Search targets depend on the current
policy and approximate search values. Thus a lower current-generation training
loss does not imply higher held-out completion, and the monotonic improvement
argument for exact policy iteration does not apply.

The implementation makes this especially sensitive:

- `hierarchical_curriculum` collects 32 new cases per generation, and
  `hierarchical_generation` trains on that collection alone for 200 epochs.
  The 128-case rolling statistic controls promotion but is not a replay buffer.
- Every root receives a policy target, including roots whose episode ends
  without a validated factory. Later completion supervises value outputs, but
  does not qualify the root policy loss.
- Gumbel root search considers 16 candidates out of roughly 600-700 available
  macro actions in the sampled generation-49 and generation-84 collections.
  Its completed-Q target assigns unvisited actions an imputed value. In those
  collections, mean policy-target mass on unvisited actions is 0.642 and 0.658
  for roots of successful episodes, and 0.726 and 0.745 for failed episodes.
  The failed-root targets are especially diffuse: their mean maximum action
  probability is 0.075 and 0.066, respectively.
- Action choice is discrete. Small ranking changes can alter the 16 root
  candidates or their elimination order; a different early placement or route
  changes the later root states and can end at `no-macro-actions`. All 90 final
  held-out failures had that termination.

The earlier curriculum failed through a sharply concentrated outer prior. This
corrected run is different: retained initial-root normalized entropies remain
near 1.0. A diffuse policy can still be unstable when the relative ranking of
many near-tied actions moves between checkpoints. The cross-checkpoint and
output ablations support that mechanism without requiring a single-action
collapse. Large reported preclip gradient norms show active clipping but do not
by themselves isolate the cause.

Oscillation is **possible under these update and search rules, not
mathematically inevitable**. A stable learner could improve on this task; this
run provides no monotonicity guarantee and empirically fails to retain hard
cases.

## Provenance, reproduction and next gate

The retained executable is
`~/Srv/aic-gumbel-curriculum-20260922/bin/aic-cli`, SHA-256
`6b0c7fad7869f63da3859c35fa077500887a2fd1835067fbcc8871823bb5c747`.
The tracked search configuration SHA-256 is
`e503a8929093b2551b5831561c7e09997e56752b7884bee693df5e2679e9269f`.
The compact evidence file records the held-out panel, checkpoint, ablated
checkpoint and raw-report hashes and each case outcome. For a crossed cell,
from the retained server checkout with the LibTorch environment in
[`training-server.md`](../maintenance/training-server.md):

```sh
bin/aic-cli learning hierarchical-search-harness \
  --config docs/benchmarks/hierarchical-curriculum/search.json \
  --panel inputs/heldout-panel.json \
  --lower-training-output artifacts/hierarchical-curriculum-v1/generations/000049/training/lower \
  --outer-training-output artifacts/hierarchical-curriculum-v1/generations/000084/training/outer \
  --catalog-root data/game/normalized \
  --output artifacts/hierarchical-stability-replay/lower49-outer84.json \
  --device cuda:0
```

Swap the two generation numbers for the other crossed cell. The ablated
checkpoints are reconstructed by copying generation 84's outer `config.json`
and `parameter-order.json`, then zeroing exactly the named final-layer tensors
in a copy of its safetensors state. The 22-case panel is the subset whose
outcomes are true in `lower49_outer49` and false in `lower49_outer84` in the
compact evidence file. No original checkpoint was modified.

The next bounded comparison should use the fixed **training-only** diagnostic
panel and new, disjoint training panels. It should isolate whether retained
successful roots, outcome-qualified policy targets, or smaller outer updates
stabilize the policy while preserving easy controls. Declare the stability
threshold before running it. Do not resume the stopped 88-generation run or
launch an ultra-long curriculum from its current learner.

## Cleanup

After verifying the tracked compact evidence against its server copy, the
diagnostic directory
`~/Srv/aic-gumbel-curriculum-20260922/artifacts/hierarchical-stability-diagnosis-20260923`
was removed. It contained the two crossed raw reports (610,931,011 and
608,249,952 bytes), two ablation raw reports (103,222,918 and 99,704,503
bytes), the derived 22-case panel, two modified checkpoint copies and the
verified compact-evidence copy. Original curriculum checkpoints, held-out
reports, input panel and retained executable were untouched. Filesystem
available space increased from 177,474,875,392 to 178,897,395,712 bytes.
