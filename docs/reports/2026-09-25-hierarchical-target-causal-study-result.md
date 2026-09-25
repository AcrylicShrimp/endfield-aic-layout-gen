# Hierarchical search-target oscillation: causal study synthesis

Status: the three declared uncertainties were investigated through frozen
search, forced-action continuations, independent repetition, target scoring,
and paired learner screens and fresh-collection confirmation. No production
search or training setting was changed.

## Findings

| Question | Evidence | Verdict |
| --- | --- | --- |
| Does the 16-action root cap exclude useful first actions? | The fixed-seed width/budget matrix never selected outside the original 16 in 48 cases. Independent forced continuations nevertheless confirmed four outside-action advantages over the original choice in three cases; each original choice succeeded 0/6 times while those outside choices succeeded 4/6 or 5/6. Under two other root seeds, 95/96 first choices lay outside the original seed's visited 16. | **Yes, for some sampled cases.** The frequency among roughly 2,734 possible macros and the best cap are unresolved. Wider search under one seed alone did not recover these actions reliably. |
| Does searched Q predict independent continuation? | Among 14 independently confirmed winner/loser pairs where both original-root actions were visited, larger Q pointed to the winner 3 times, loser 10 times, and tied once. The 128-candidate/512-simulation root gave 13/20/4 over 37 confirmed both-visited pairs. The 512-simulation continuation control improved absolute completion by 11/144 case-seed trials, but 22/48 cases still changed success across seeds. | **Not a reliable full-continuation completion ranking in this sample.** Search Q averages short-horizon leaf values; unfinished leaves receive construction progress. A conditional footprint signal was too uncertain to rescue it as a quality predictor. |
| Does the full-action target dilute learning and cause the churn? | The current first-root target placed 99.47% of mass on unvisited actions and had a 99.38% effective action fraction. Sharper or visited-focused targets reduced dilution, but all three worsened pairwise loss against 66 independently confirmed action comparisons. A paired frozen-collection learner screen and three-stream, three-generation fresh-collection confirmation failed their predeclared retention gates. | **Dilution is measured; a simple causal repair is not established.** The tested half-visited first-root stress target was not promoted. |

The supporting stage reports are the
[candidate-width matrix](2026-09-25-hierarchical-candidate-width-result.md),
[primary continuations](2026-09-25-hierarchical-first-action-result.md),
[independent repetition](2026-09-25-hierarchical-continuation-replication-result.md),
[512-simulation volatility control](2026-09-25-hierarchical-continuation-budget-result.md),
[original-seed outside budget](2026-09-25-hierarchical-outside-budget-result.md),
[independent outside recheck](2026-09-25-hierarchical-outside-recheck-result.md),
[search-seed target audit](2026-09-25-hierarchical-target-seed-result.md),
[offline target scoring](2026-09-25-hierarchical-target-contrast-result.md),
[frozen learner screen](2026-09-25-hierarchical-target-learner-screen-result.md),
and [fresh-collection learner confirmation](2026-09-25-hierarchical-target-learner-confirmation-result.md).

## What explains the nonmonotonic curve?

There is no mathematical rule requiring each generation to improve. The
implemented loop samples a small, seed-dependent root candidate set; scores
actions from finite-depth search with a progress heuristic; trains on a
nearly uniform full-action target; then uses the changed model to collect
the next generation's data. Each stage can change which cases succeed. This
is a concrete route to gains and losses, not a proof that any one stage alone
caused the historical curriculum curve.

The controlled tests narrow that picture. Changing only root Gumbel seed
changed the selected first action in all 48 frozen cases, although the
current nearly uniform target distribution hardly changed. Increasing the
same forced-action continuation from 128 to 512 simulations helped some
cases but left substantial seed sensitivity. Of ten outside actions that
looked favorable under the original three seeds, all ten still led under
those seeds at 512 simulations, but under three independent 512-simulation
seeds only four led, three trailed, and three tied. More simulations did not
make these tentative rankings reproducible.

The learner experiment directly demonstrates the feedback step. Generation
1 used byte-identical search collections within each pair, differing only
in selected policy targets. In generations 2 and 3, the paired arms used the
same 32 problems with matching first states, but their selected first
actions matched on only 1/0/2 of 32 cases across streams at generation 2
and 0/0/0 at generation 3. The first-root target change can therefore
strongly redirect later data collection. Yet the current-target controls
also oscillated: S1 completed 23 → 30 → 21 cases, S2 22 → 21 → 19, and S3
25 → 23 → 17. In S1's current arm, the last transition lost 21 previously
solved cases and gained 12 others. Simple aggregate completion curves hide
this case replacement.

The half-visited stress contrast finished with 21/21/25 completions versus
current's 21/19/17 across streams, but improved retention of the starting
successes by only 1/3/2 cases. Its mean improvement of 2.0 was below the
predeclared 3.0-case requirement; the first-generation direct comparison
also had fewer completions in all three streams. The outcome does not
support changing training to this target. Losses computed on different
target distributions are not comparable across arms, even when both losses
decrease within their own arms.

## Limits and next research decision

These are bounded experiments on 48 frozen training-source problems,
prespecified sampled first actions, six downstream seeds split into two
triplets, and a separate 64-case training-partition diagnostic. The
confirmed-action set is selected for reproducible large differences, not a
random estimate of all macros. Forced actions change later state hashes and
therefore later Gumbel draws even when numeric seeds match. The learner
confirmation has only three independent streams, freezes the lower model,
and does not use the untouched final held-out test. It can reject the tested
promotion gate, not prove that target construction is irrelevant in general.

The next bounded research question should test a first-action value or
ranking tied to longer validated continuation, while separately measuring
candidate recall under multiple root seeds. A target transform should be
selected on independent action outcomes and then tested for case-level
retention with fresh on-policy data. Increasing simulations or sharpening
the existing short-horizon Q by itself is not justified as a production
fix. A lower-training restart and untouched held-out evaluation remain
required before adopting any learner change.

All executable source identities, configurations, model/report hashes,
case-level masks, W&B acknowledgements, and byte-precise cleanup records
are retained in the linked stage protocols and reports. The release binary
for frozen-search stages had SHA-256
`e594b5425074a03731524d8c91991126232d840ef44a86e8b0630e1f16800fdf`.
