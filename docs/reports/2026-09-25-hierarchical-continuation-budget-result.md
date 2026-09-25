# Forced-action continuation budget control

Status: the three declared 512-simulation cells completed. This is a separate
volatility control motivated by the [primary continuation](2026-09-25-hierarchical-first-action-result.md),
not a candidate-width or learner intervention.

## Method

The same original first action was forced for each of 48 frozen cases under
three downstream root seeds. The native prefix, first action, resulting state,
lower and outer weight bytes, 16 root candidates, eight guaranteed anchors,
23 remaining moves, and every search control other than simulations per move
matched the 128-simulation continuation. Every paired first-state hash matched
across both budgets and all seeds. Increasing only the subsequent search
budget fourfold isolates whether finite continuation search is responsible
for the large seed-to-seed outcome variation.

## Result

| Downstream seed | 128 successes | 512 successes | Newly successful at 512 | Lost at 512 |
| --- | ---: | ---: | ---: | ---: |
| 20260922 | 19/48 | 22/48 | 3 | 0 |
| 20260923 | 23/48 | 26/48 | 6 | 3 |
| 20260924 | 22/48 | 27/48 | 6 | 1 |

Across the same 48 cases, the three-seed success count improved for 12 cases,
worsened for 2, and stayed equal for 34. The mean paired completion increase
was 11/144 case-seed trials, or 7.6 percentage points. A case-cluster
bootstrap over these 48 fixed cases gave a descriptive 95% interval of
2.8–13.2 points; it does not account for the uncertainty from sampling only
three root seeds or establish population-wide benefit. The increase in total
success is credible for this panel, but the second and third seeds still lost
some originally successful cases. More simulations did not give per-case
monotonicity.

The number of cases that changed success across the three seeds fell from
26/48 at 128 simulations to 22/48 at 512. Pairwise seed disagreements were
`20,17,15` at 128 and `18,13,13` at 512, averaging 17.3 versus 14.7 cases.
The reduction is modest. Six mixed cases became stable and two stable cases
became mixed; a case-cluster bootstrap interval for the fraction of mixed
cases removed spans -2.1 to +20.8 percentage points. Thirteen cases were
successful under all three 512-simulation seeds, up from seven; 13 failed
under all three, down from 15. The 512-simulation continuation still changes
success with seed for 22 cases, so fourfold budget does not resolve the
instability.

This suggests that 128 simulations contribute to missed completions, while
the remaining large seed sensitivity needs separate explanation. Search Q and
first-action quality remain unjudged until the independent-seed replication
of the primary action comparisons. No search budget is promoted for training
from this bounded control.

## Evidence and reproduction

The [protocol](2026-09-25-hierarchical-continuation-budget-protocol.json),
[progress and raw report hashes](2026-09-25-hierarchical-continuation-budget-progress.json),
and [case-level summary](2026-09-25-hierarchical-continuation-budget-summary.json)
retain exact inputs and paired outcomes. Experiment source commit
`32d3cbea51b81a24303d0ef1ce3d3d5be17071f8`, runner SHA-256 in the
protocol, release binary SHA-256
`e594b5425074a03731524d8c91991126232d840ef44a86e8b0630e1f16800fdf`,
and tracked per-seed search and outer configurations identify the executable
method. Three integrated searches took 1,191 seconds and wrote 295,242,324
raw report bytes. No training occurred, so W&B tracking was not required.
After the package was verified and committed, the
[cleanup manifest](2026-09-25-hierarchical-continuation-budget-cleanup.json)
recorded 590,929,906 removed file bytes, including the now unneeded three
primary rank-zero reports, and server free space rising from 175,857,012,736
to 176,447,995,904 bytes. The full primary summary and candidate panels
remain temporarily for independent replication and target analysis.

From the closed primary continuation inputs under `~/Srv`:

```bash
python3 tools/factory-edit-pilot/hierarchical_continuation_budget_control.py prepare \
  --output artifacts/target-causal-study \
  --experiment-commit 32d3cbea51b81a24303d0ef1ce3d3d5be17071f8
python3 tools/factory-edit-pilot/hierarchical_continuation_budget_control.py run \
  --output artifacts/target-causal-study
python3 tools/factory-edit-pilot/hierarchical_continuation_budget_control.py summarize \
  --output artifacts/target-causal-study
```
