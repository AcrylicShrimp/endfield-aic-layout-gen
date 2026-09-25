# Independent-seed outside-action continuation at 512 simulations

Status: all 18 prespecified cells completed. This is the second triplet for
the **same ten pairs** from the
[original-seed 512-simulation check](2026-09-25-hierarchical-outside-budget-result.md).
No target or model was changed.

## Method

The same nine frozen cases, ten candidate first-action witnesses, nine
original first-action witnesses, lower and outer weights, 16 downstream
candidates, and 23 remaining moves were used. Later search had 512
simulations per move under independent root seeds 20260925–20260927. Every
forced first-state hash matched the primary continuation for its action.
The [protocol](2026-09-25-hierarchical-outside-recheck-protocol.json) fixes the
selection to the earlier ten pairs, and the
[progress](2026-09-25-hierarchical-outside-recheck-progress.json) hashes all
raw reports.

## Result

| Downstream search and seeds | Candidate better | Baseline better | Tied | Candidate ahead by at least 2/3 |
| --- | ---: | ---: | ---: | ---: |
| 128, original seeds 22–24 | 10 | 0 | 0 | 10 |
| 128, independent seeds 25–27 | 3 | 5 | 2 | 0 |
| 512, original seeds 22–24 | 10 | 0 | 0 | 7 |
| 512, independent seeds 25–27 | 4 | 3 | 3 | 0 |

For the independent 512-simulation triplet, one baseline action led by at
least two successes; no candidate did. These ten pairs include two candidates
from one case and were selected for having won the original 128-simulation
triplet, so the table is descriptive, correlated, and selection-biased. The
[case-level summary](2026-09-25-hierarchical-outside-recheck-summary.json)
retains every comparison and seed outcome.

Fourfold downstream search did **not** make the original-seed candidate
advantages repeat under independent seeds. In case S1-22498, both outside
candidates won 3/3 versus 1/3 at 512 under original seeds, yet lost 1/3 and
2/3 versus 3/3 under independent seeds. Case S2-17554 changed from 3/3
versus 0/3 under original seeds to 1/3 versus 0/3 under independent seeds.
Thus a useful outside action can be missed by the original cap in some cases,
as the separate four independently confirmed primary pairs showed, but these
additional ten outside advantages are **not established**. More downstream
simulations improve some absolute outcomes in the broader 48-case control but
do not remove this seed sensitivity.

The same numeric seed does not give two actions an identical later random
trajectory: each forced action changes the state hash used to draw later
Gumbels. The paired case and seed design controls the initial problem and
search settings, but does not create common random numbers after the first
action. Three seeds per triplet cannot estimate a small success-probability
difference precisely. We therefore retain the independently confirmed
128-simulation pairs as a guarded offline target-scoring set and use a
separate paired learner experiment for target causality.

## Provenance and closeout

Source commit `f8802d3ecae2b625089c491095118e3eaeba5aee`, tracked runner
SHA-256 `76f9a73d5556a65220d8b2f3b61378d651813f7b6449957bf205c4e7f5208f2a`,
tracked per-seed search and outer configurations, and the protocol's binary
and checkpoint hashes identify the execution. Eighteen integrated cells took
516 wall seconds and wrote 108,508,248 report bytes. No model was trained, so
W&B tracking was not needed. The
[cleanup manifest](2026-09-25-hierarchical-outside-recheck-cleanup.json)
records the removed raw paths and disk counts after this report was verified.

From the retained frozen inputs under `~/Srv`, with the documented libtorch
environment exported:

```bash
python3 tools/factory-edit-pilot/hierarchical_outside_budget_recheck.py prepare \
  --output artifacts/target-causal-study \
  --experiment-commit f8802d3ecae2b625089c491095118e3eaeba5aee
python3 tools/factory-edit-pilot/hierarchical_outside_budget_recheck.py run \
  --output artifacts/target-causal-study
python3 tools/factory-edit-pilot/hierarchical_outside_budget_recheck.py summarize \
  --output artifacts/target-causal-study
```
