# Independent first-action continuation replication

Status: all 30 independent-seed cells completed. This extends the
[primary forced-action result](2026-09-25-hierarchical-first-action-result.md)
after its unusually large search-seed variation. No model was trained and no
production search setting changed.

## Method and validity

The primary three-seed screen had 251 action pairs with a completion-count
difference of at least two, spread across 35 of the 48 frozen cases. The
[predeclared replication rule](../designs/2026-09-25-hierarchical-target-causal-study.md)
replayed **every prespecified action** in those 35 cases, not just the apparent
winner, under new downstream root seeds 20260925–20260927. Search remained at
16 candidates, 128 simulations per move, and 23 remaining moves with the same
generation-87 weights. The original first action and prefix were forced again.
There were 278 distinct case/action pairs, each with three new continuations.
All 834 forced-action first-state hashes matched the primary result. A pair
was confirmed only if the same action won by at least two of three completions
in both independent seed triplets. This is a reproducibility filter, not a
formal confidence interval or an optimal-action oracle.

## Result

Only **66 of 251** primary clear pairs were confirmed, in 22 cases. Confirmed
pair counts were 26, 16, and 24 across S1, S2, and S3. The high attrition
shows that three-seed action labels are unstable and supports keeping the
target-selection gate tied to independent repetition.

| Confirmed comparison with original first action | Candidate better | Candidate worse | Cases with candidate better |
| --- | ---: | ---: | ---: |
| Other action inside original 16 | 4 | 3 | 3 |
| Action outside original 16 | 4 | 2 | 3 |

The four confirmed outside advantages occur in three cases; one case has two
better outside actions. The original selected action completed **0/6** times
in each of these three cases, while the corresponding outside actions
completed 4/6 or 5/6. This establishes that the original 16 missed some useful
first actions in this sampled set. It does not estimate the prevalence of
such actions among roughly 2,734 available macros. Of the 14 primary outside
advantages over the original choice, four confirmed, two reversed clearly,
and eight remained unresolved under the new triplet. The changed claims and
unresolved comparisons trigger the study's separate 512-simulation
continuation escalation on those ten candidate/action pairs.

The searched Q ranking remains concerning after the reproducibility filter.
Among 14 confirmed winner/loser pairs where **both** actions were visited in
the original 16/128 search, higher Q pointed to the winner three times, the
loser ten times, and tied once. Eleven independent cases contribute; by
case-level majority, seven favor the wrong Q direction, one the right
direction, and three tie. In the 128-candidate/512-simulation root search,
37 confirmed pairs were both visited: higher Q favored the winner 13 times,
the loser 20 times, and tied four times. These samples are small and pairs
within a case share outcomes, but they do not support treating the recorded Q
as a reliable full-continuation success estimate. The code's Q averages
finite search leaf values; an unfinished leaf receives construction-progress
value. One confirmed outside winner was visited once by the widest root
search and received Q 0.182 versus Q 0.547 for its selected original action,
even though independent continuations completed 5/6 versus 0/6. The other
three confirmed outside winners were not visited by that wide search.

Conditional layout quality tells a different, weaker story. Among 233
same-seed comparisons in 26 cases where both original-16 actions completed,
the higher-Q action used a smaller footprint 116 times, a larger footprint
99 times, and the same footprint 18 times. The mean of case-level footprint
differences (higher Q minus lower Q) was -7.5 cells; a case-cluster bootstrap
95% interval was -24.2 to +7.6 from 20,000 resamples with seed 20260925.
The corresponding transport-cell interval was -5.9 to +7.3. These
comparisons condition on both actions succeeding,
so they cannot establish Q's effect on overall quality. They show no clear
quality benefit large enough to rescue Q as a completion predictor.

## Evidence, limits, and next decision

The [protocol](2026-09-25-hierarchical-continuation-replication-protocol.json),
[progress and raw hashes](2026-09-25-hierarchical-continuation-replication-progress.json),
[case-level confirmed pairs](2026-09-25-hierarchical-continuation-replication-summary.json),
and [Q, candidate, and quality analysis](2026-09-25-hierarchical-continuation-replication-analysis.json)
retain the decision basis. Source experiment commit
`820a13dac399442840db7e859f1ebf8c83b140af`, runner hash in the
protocol, and release binary SHA-256
`e594b5425074a03731524d8c91991126232d840ef44a86e8b0630e1f16800fdf`
identify the executable method. The case-level conditional-quality analysis
was expanded in commit `c3d1150e` without changing searches or confirmations.
Thirty integrated cells took 2,935 seconds
and produced 1,704,127,658 raw report bytes. This bounded diagnostic did not
need W&B tracking.

The immediate first-action evidence is strongest for three cases with missed
outside actions and for poor Q/completion alignment within the sampled confirmed
pairs. Both remain conditional on the frozen model, cases, candidate-selection
scheme, and downstream search. We will test whether fourfold continuation
budget resolves the ten changed outside comparisons before closing candidate
coverage and Q diagnosis. Target transformations may then be scored only
against the 66 independently confirmed pairs; learner effects require their
own paired training and retention check.

From the closed primary continuation inputs under `~/Srv`:

```bash
python3 tools/factory-edit-pilot/hierarchical_continuation_replication.py prepare \
  --output artifacts/target-causal-study \
  --experiment-commit 820a13dac399442840db7e859f1ebf8c83b140af
python3 tools/factory-edit-pilot/hierarchical_continuation_replication.py run \
  --output artifacts/target-causal-study
python3 tools/factory-edit-pilot/hierarchical_continuation_replication.py summarize \
  --output artifacts/target-causal-study
python3 tools/factory-edit-pilot/analyze_hierarchical_continuation_replication.py \
  --output artifacts/target-causal-study
```
