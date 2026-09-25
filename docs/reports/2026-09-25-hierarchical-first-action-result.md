# Forced first-action continuation result

Status: all 30 primary cells completed. This is the first-action stage of the
[target causal study](../designs/2026-09-25-hierarchical-target-causal-study.md),
not a learner intervention or production search change. A separately declared
independent-seed replication and 512-simulation volatility control remain
necessary before judging action quality or Q ranking.

## Question and method

The candidate-width study found that every wider search still selected its
first action from the original 16. Did those 16 exclude better actions, and
did searched Q rank sampled actions by actual continuation? For each of 48
frozen train-source cases, exact first-action witnesses were prespecified from
the seven width arms, high and low Q among the original 16, the best searched
guaranteed anchor, high-prior actions outside 16, a high-Q action outside 16
visited by the 128/512 arm, and a deterministic outside sample. Deduplication
left 379 distinct case/action pairs, or 7–10 actions per case. Each was applied
to the identical native prefix and continued for 23 moves with 16 candidates
and 128 simulations per move under root seeds 20260922, 20260923, and 20260924.
The frozen lower and outer weights, search settings other than root seed,
catalog, and action witnesses were held fixed.

The 48 original first choices under seed 20260922 reproduced the original
baseline search after the first move exactly: every later root's state hash and
selected action, final status, completed connections, quality fields, and move
count agreed. Across all 379 forced actions, the first state hash agreed
between all three seeds. This establishes that the observed seed variation
arose after the controlled first action, rather than from a different action
or an invalid prefix.

## Primary observations

| Comparison with the original selected action | Distinct candidates | Better by at least 2/3 successes | Worse by at least 2/3 | Unresolved |
| --- | ---: | ---: | ---: | ---: |
| Other actions inside the original 16 | 139 | 18 | 14 | 107 |
| Actions outside the original 16 | 192 | 14 | 17 | 161 |

Eleven cases had at least one outside candidate that met the primary better
rule; 13 cases had an inside candidate that did. These are **screening counts**,
not estimates of how common better actions are: each case tested several
purposefully selected candidates, and the three-seed continuation is noisy.
Among the prespecified outside groups, the highest-anchor action had 6 better
and 1 worse, the highest Q visited outside action had 1 better and 9 worse,
and the deterministic outside sample had 3 better and 6 worse. These contrasts
raise a specific concern about searched Q; independent repetition will decide
whether their direction survives.

The search-Q diagnostic also disagreed with the primary continuation labels.
For pairs where both actions were visited in the original 16/128 search and
one had at least two more successes, higher Q favored the winner in 17 of 65
pairs, tied in 11, and favored the loser in 37. The corresponding 16/512 cell
gave 21 concordant, 7 tied, and 37 discordant of 65. The 128/512 cell had 42
concordant and 16 tied of 121 compared pairs. Each statistic reuses cases and
three-seed labels, so none is an independent accuracy estimate. The recorded
Q is the mean of finite search leaf values, whose unfinished-state component
is construction progress. It need not equal the probability that a later
23-move search fully validates a layout.

The common first action itself showed large downstream seed variation. It
completed 19, 23, and 22 of 48 cases under the three seeds. Seven cases
succeeded under all three, 15 failed under all three, and 26 changed outcome
with seed. Pairwise success disagreements were 20, 17, and 15 cases. Across
all 379 forced actions, the three seed-pair disagreement counts were 138,
133, and 139. A one-run win or loss is therefore weak evidence of first-action
quality. Conditional footprint and transport comparisons are retained in the
compact analysis only for seeds where both paired actions completed; their
selection on success precludes an overall quality claim.

## Evidence and decision

The [protocol](2026-09-25-hierarchical-first-action-protocol.json),
[per-cell progress and raw hashes](2026-09-25-hierarchical-first-action-progress.json),
and [compact case and group evidence](2026-09-25-hierarchical-first-action-compact.json)
record the inputs and outputs. The exact action witnesses are the registered
[S1](../benchmarks/hierarchical-target-causal/continuation-actions-s1.json),
[S2](../benchmarks/hierarchical-target-causal/continuation-actions-s2.json), and
[S3](../benchmarks/hierarchical-target-causal/continuation-actions-s3.json)
source assets. The full continuation summary and raw reports are temporary
server artifacts; their hashes bind them to this compact package while the
follow-up uses them. The source experiment commit is `cca8d71`, and the
release executable SHA-256 is
`e594b5425074a03731524d8c91991126232d840ef44a86e8b0630e1f16800fdf`.
The 30 integrated searches took 3,755 seconds and wrote 2,291,753,859 raw
report bytes. This bounded diagnostic did not train a model, so W&B tracking
was not required.

The large seed variation triggered an [independent replication rule](../designs/2026-09-25-hierarchical-target-causal-study.md)
before the remaining alternative-action results were inspected. There are
251 primary clear action pairs in 35 cases; all prespecified actions in those
cases will be repeated under three new seeds. Only pairs with the same clear
winner in both triplets can select a policy-target contrast. Separately, the
same original first action will be continued with 512 simulations under the
original three seeds to check whether added search budget reduces outcome
volatility. Until those controls finish, neither candidate coverage nor Q
misranking is a settled causal explanation.

To reproduce from the frozen candidate-width inputs and tracked continuation
configurations under `~/Srv` with the documented LibTorch environment:

```bash
python3 tools/factory-edit-pilot/hierarchical_first_action_continuation.py prepare \
  --output artifacts/target-causal-study --experiment-commit cca8d71
python3 tools/factory-edit-pilot/hierarchical_first_action_continuation.py run \
  --output artifacts/target-causal-study
python3 tools/factory-edit-pilot/hierarchical_first_action_continuation.py summarize \
  --output artifacts/target-causal-study
python3 tools/factory-edit-pilot/analyze_hierarchical_first_action_continuation.py \
  --output artifacts/target-causal-study
```
