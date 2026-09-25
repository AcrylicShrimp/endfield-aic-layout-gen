# Outside-candidate continuation at 512 simulations: original seeds

Status: all 18 prespecified cells completed. This bounded diagnostic uses the
same three downstream root seeds as the primary 128-simulation screen. It is
not an independent replication of that screen.

## Method and result

The [protocol](2026-09-25-hierarchical-outside-budget-protocol.json) selected
all ten outside-original-16 candidate/baseline pairs in nine cases that won
the primary three-seed 128-simulation comparison by at least two completions
but did not repeat that margin under independent seeds 20260925–20260927.
The exact first action was forced for each arm; only later-move simulations
changed from 128 to 512, with 16 candidates and 23 remaining moves. Seeds
20260922–20260924 match the primary screen. Every forced first-state hash
matched its prespecified primary hash across all three seeds and both actions.

All ten candidates completed more often than their case's original action at
512 simulations under these seeds: seven by at least two of three trials and
three by one trial. Counts across the ten comparisons were 26/30 candidate
trials and 7/30 baseline trials; the baseline is repeated in the one case
with two candidates, so these are pair counts, not independent case trials.
For the nine distinct baseline actions, success totaled 6/27 trials. The
[case-level summary](2026-09-25-hierarchical-outside-budget-summary.json)
retains every seed outcome and connection progress.

This agrees in direction with the original 128-simulation seed triplet but
does not explain why the independent 128-simulation triplet failed to confirm
these advantages. The high-budget result is confounded with reuse of the
favorable original root seeds. We therefore cannot label all ten pairs
robust winners or attribute the reversal to simulation budget. The next
bounded check repeats these same ten pairs at 512 simulations under the
independent seeds, without changing the selected set.

## Provenance and closeout

The [progress](2026-09-25-hierarchical-outside-budget-progress.json) retains
SHA-256 hashes for all 18 raw reports. They took 522 integrated wall seconds
and wrote 111,882,652 report bytes. Lower and outer checkpoint and release
binary hashes are in the protocol. The tracked runner at source commit
`39ef35fb84c976d026bd9301f4fe10c37246d972` has SHA-256
`9cc15b3d2bd5b5323df7f75d77e72f7d0b459bd90eeec65fa6bdfd6e085e870e`.
The protocol's `experiment_commit` field mistakenly contains the short string
`51b9478d`, which is **not** the source identity; the tracked commit and runner
hash above reconstruct the executed method. The first launch also lacked the
documented `LD_LIBRARY_PATH` and failed before producing a report; the same
cell was then run successfully with the server's libtorch environment.
No model was trained, so W&B tracking was not required. The raw reports are
removed after this compact package is verified; the
[cleanup manifest](2026-09-25-hierarchical-outside-budget-cleanup.json)
records the paths and disk counts.

From the retained frozen inputs under `~/Srv`, with the documented libtorch
environment exported:

```bash
python3 tools/factory-edit-pilot/hierarchical_outside_budget_escalation.py prepare \
  --output artifacts/target-causal-study \
  --experiment-commit 39ef35fb84c976d026bd9301f4fe10c37246d972
python3 tools/factory-edit-pilot/hierarchical_outside_budget_escalation.py run \
  --output artifacts/target-causal-study
python3 tools/factory-edit-pilot/hierarchical_outside_budget_escalation.py summarize \
  --output artifacts/target-causal-study
```
