# Independent success-scorer adoption review

> Withdrawn from scientific selection on 2026-09-14 at the user’s request.
> Historical record only; use the [replacement quality study](../designs/2026-09-14-quality-weighted-replay-study.md) for new selection evidence.

## Decision and scope

**BLOCK immediate application to the running long training.** Two independent,
fresh-context reviewers examined the implementation at
`43ea881020bd0e4ef521fdc14bedd86f4f215ced`: one tested human-intent alignment and
geometry, and one tested production retention, sampling, and recovery readiness.
Both reached BLOCK for adoption. The offline calculation contract passed; the
second reviewer classified that narrower result as PASS_WITH_NOTES.

The parent independently reran both bounded probes, checked the production
source, inspected the additional actual-example figure, and checked historical
eviction events. No scorer implementation defect was established within its
documented frozen-pool contract. The remaining blockers require a production
policy definition and integration work beyond this review. The review therefore
ends after one round with two reviewers. No live learner, training configuration,
reward, checkpoint, or sampling behavior was changed.

The input and configuration are bound by the existing
[provenance](2026-09-14-declarative-success-scoring-provenance.json); their hashes
were verified. The [original experiment](2026-09-14-declarative-success-scoring.md)
defines the formulas and the hypothetical selection budgets.

## Human-intent findings

All 4,096 feature rows and all 42 stored profile/stratum/budget selections
reproduced. At small-stratum budget 128, adding capped exterior scoring to
3-point rarity replaces 12 deletion-only segments with 12 facility-edit
segments, without losing any facility-edit segment selected by rarity alone.
The actual-gain cap also corrects a conspicuous magnitude mismatch: `28c3cd68`
has 1.96% actual improvement but 16.99% exterior signal; its rank falls from 23
uncapped to 1,110 capped. These are verified ranking benefits.

However, category protection is not demonstrated causal credit. Rarity gives
every place-containing segment the same stratum-specific offset. Geometry
counts signed changes across `place` transitions and excludes band transforms
(`diagnostics/declarative_success_scoring.py`, `features` and `score`). A bounded
mathematical probe with identical initial/final facility bodies and stated gain
produces the following capped scores:

| Action construction | Score |
| --- | ---: |
| Deletion | 0.166667 |
| Facility out-and-back, then the same deletion | 0.195539 |
| Band expansion, facility moved back, then the same deletion | 0.362205 |

The first extra score is rarity; the second also credits contraction whose
expansion came from a band action. This verifies feature-level ambiguity, **not
a native-valid exploit**: the synthetic counterfactuals were not executed with
complete transport and port state. Their native realizability and contribution
to learning remain inconclusive. The cap bounds magnitude, not attribution.

Actual successful sequences also include outward movement. `07db6d65` moves a
facility outward by two tiles before deleting a band, reducing area 195 to 165.
`49f420e2` includes outward horizontal movement, rotation, and port selection,
reducing area 2,610 to 2,565. Both spatial signals are zero. Neither geometry
picture proves those moves were necessary, but mandatory inward movement would
exclude recorded successes. The current optional bonus does not exclude them:
rarity moves the large example from area-only rank 1,222 to rank 15, unchanged by
the exterior bonus.

Generalization is unproven. All 77 small facility-edit segments use only two or
three facilities. Both spatial signals are zero in 35/77 small and 5/6 large
segments; exterior signal is zero in all six large segments. The 4,096 retained
segments cover 407 episodes and 368 geometries, with no already-evicted or
unsuccessful examples. This evidence supports an experimental category
preference, not a general detector of sophisticated or necessary preparation.

## Production-readiness findings

The online buffer admits new unique successes at true terminal and evicts through
`declarative_replay_retention.ReplacementIndex`. It uses observed SIL contribution
for entries with at least eight draws and two observed updates, otherwise the
oldest entry in the chosen stratum. Offline top-K ranking does not define how
its score interacts with that fallback, new admission, dynamic rarity, mature
utility, or ties.

At frozen update 1,410, no entry satisfies maturity; the maximum draw count is
seven. Calling the actual replacement index before any further observation
selects the oldest fallback in both strata. This does not imply that the next
real update is entirely FIFO: `Learner.update` records new observations before
ingestion and can produce mature entries.

A separate read-only count of actual seed-29 update events confirms the dominant
path over updates 1,379 through 1,410:

| Recorded eviction reason | Count | Fraction |
| --- | ---: | ---: |
| `insufficient_observations_oldest` | 4,457 | 97.44% |
| `lowest_observed_contribution` | 117 | 2.56% |

Thus adding a score only to mature-entry comparisons would miss the dominant
observed eviction path. This window is evidence about current operation, not a
claim that the same ratio holds throughout training.

At the actual frozen capacity of 2,048 sequences per stratum, all three profiles
(area, rarity, capped exterior) select the same 83 facility-edit segments and
4,606 labels. The production sampler draws four uniformly per stratum when both
are available. An actual-function CPU probe of 1,000 minibatches gives 151 small
and 11 large facility-edit draws among 8,000 draws. Adding score metadata leaves
every draw identity unchanged. Example `9b14fd2f` entered at update 1,396 and had
zero uses at 1,410. Better survival could improve lifetime exposure, but neither
the metadata nor a tighter static top-K screen demonstrates that improvement.

Online execution and migration remain absent. Normal restore validates protocol
identity; `declarative_execution_migrate.compatible` requires unchanged `success`
configuration. An explicit supported policy transition must preserve optimizer
state, RNGs, ongoing episodes, labels, and terminal returns. The scorer's
incremental area gain must not replace SIL's terminal return or positive
return-minus-critic weighting.

Storage and speed require an integrated check. At budget 128 per stratum,
area-only still references 32 of 35 source chunks, and capped exterior references
34. Fewer selected segments do not imply proportional disk savings. Local probes
took approximately 0.041 seconds for features, 0.002 seconds for replacement
index construction and one selection, and 0.30 seconds for 1,000 sampling batches.
These exclude capture, model execution, publication, compression, and disk I/O.
They establish neither a production speedup nor a storage regression.

## Minimum evidence before adoption

1. Define whether the policy explicitly protects the facility-edit category or
   attempts to identify useful preparation. Keep inward movement optional and
   use complete native states for a few band/pose reversal counterfactuals if
   geometric contribution is part of the intended claim.
2. Define the eviction rule including the currently dominant immature fallback.
   Run a bounded chronological admission/eviction comparison at the real
   4,096-sequence and 262,144-label limits. Compare the existing policy,
   rarity-only, and capped geometry using rare-success survival, actual sampling
   exposure, ordinary deletion coverage, geometry diversity, and retained source
   bytes. Survivor-only input cannot reconstruct missing historical arrivals;
   any synthetic sensitivity test must say so.
3. After implementation, verify an explicit policy transition and a bounded
   continuation/restore/publication workflow. Preserve the indexed replacement
   path, cached replay inputs, batching, source pruning, and existing credit
   contracts. Compute admission features from available evidence once; do not
   introduce repeated capture, source scans, or full-buffer geometric rebuilds.

These are adoption gates, not authorization to modify the running training.
No new long training is needed to resolve the immediate semantic and integration
questions.

## Reproduction and verification record

Reviewer scratch inputs, scripts, full IDs, figures, and outputs are retained
locally under `artifacts/declarative-success-scoring/20260914/review/`:

```sh
tools/factory-edit-pilot/.venv/bin/python \
  artifacts/declarative-success-scoring/20260914/review/human-intent/probe.py
tools/factory-edit-pilot/.venv/bin/python \
  artifacts/declarative-success-scoring/20260914/review/training-readiness/probe.py
```

The first probe independently reconstructs features/rankings, compares selected
IDs, executes the mathematical probes, and plots the actual outward examples.
The second exercises actual production replacement and sampling functions and
counts source references. Their reports are `review.md`; numerical evidence is
`human-intent/probe-results.json` and `training-readiness/probe.json`. Outputs and
scratch scripts remain ignored local evidence, not production paths.

The historical eviction tally was read from `updates/1379.json` through
`updates/1410.json` below the source run recorded in the companion provenance.
It counts `event['reason']` for `event['kind'] == 'evicted'` in each record's
`events` array; total 4,574. It does not require learner checkpoints or source
chunk decoding.

Both reviewer worktree baselines were clean and neither changed tracked files.
The parent changed only this report. The previously completed scorer test suite
and workspace checks are recorded in the original experiment; they were not
repeated for this documentation-only review. Worktree and index documentation
hygiene checks passed for this report's commit.
