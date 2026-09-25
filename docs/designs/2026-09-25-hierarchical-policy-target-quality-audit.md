# Hierarchical search-policy target quality audit

Status: protocol fixed before audit search results. The
[update-budget probe](../reports/2026-09-25-hierarchical-update-budget-result.md)
showed large outer-policy case churn without a consistent retention benefit
from shorter training. This bounded audit inspects the teacher itself; it does
not train or select a model.

## Question and measurement boundary

The Gumbel search visits at most 16 first-root actions, but its completed-Q
policy target assigns mass across the entire root action set. Determine how
much 128-simulation target mass is on actions that received zero simulations,
and whether the first-root target remains stable when the same frozen model
and problem receive 512 simulations. A larger search budget is a stronger
measurement, not an oracle for optimal action value. Search continuation may
be discontinuous even when the target is useful.

Prepare three fresh 16-case, two-facility train-source panels with seeds
20261410, 20261430 and 20261450 and namespaces
`hierarchical-target-quality-s1/s2/s3-v1`. Keep the established sampling
config, maximum six terminals and connections, 10,000 turns, extension 32,
and canonical generation-87 lower/outer checkpoints. No panel enters training.
Run the existing eager hierarchical search harness twice per panel with
identical model weights, catalogs, panel, root Gumbel seed, search horizon,
candidate construction, and move budget. The only treatment is 128 versus
512 simulations per move. The 512 outer config changes only its embedded
search budget to satisfy the existing model/search contract; its weight bytes
must match the 128 model.

Compare only each case's **first committed root**, where the state and action
set can be matched exactly across budgets. Require identical state hashes,
revisions, and action identities for a paired comparison. A no-action case
is reported as uncovered, not silently discarded. Match action identities by
canonical serialized macro witness, not by array index alone. Preserve the
full trajectory reports only until the compact audit and hashes are verified.

## Predeclared metrics and interpretation

For each paired first root report action count, visited count, target mass on
zero-visit actions, whether target top-1 was visited, target entropy, mass and
rank assigned to the 512-simulation selected action, target top-1 agreement,
and Jensen-Shannon divergence between the 128 and 512 targets. Stratify the
mass and agreement summaries by the 128-search episode's final success or
failure. Also report both full-episode completion counts and paired gains and
losses, but do not equate search success with a unique correct root action.

Call the target **weakly grounded by sampled outcomes** if mean 128 target
mass on zero-visit actions exceeds 0.5 on at least two of the three panels.
Call it **budget-sensitive** if first-root top-1 disagreement exceeds 25% of
covered cases overall. These are diagnostic labels, not significance tests or
production gates. If both hold, investigate unvisited-action value completion
and prior mixing before changing the learner. If only one holds, report the
specific uncertainty. If neither holds, target sampling is less suspicious
and case churn may instead reflect model updates or search-path sensitivity.
Regardless of result, do not claim target correctness without a forced-action
counterfactual or independent stronger solver outcome on the same root states.

## Runtime and closeout

Run on `ashrimp-arch` under `~/Srv` after disk, GPU and CUDA checks. Use the
existing persistent search harness and six bounded panel runs, not per-action
process startup. Verify exact panel, checkpoint, executable, configuration and
raw-report hashes before analysis. Retain a compact per-case audit with source
identity, method, results, verdict, reproduction commands and W&B status if
used. Remove raw search reports, generated panels, copied checkpoints and
regenerable build cache after the closeout package is verified readable, and
record path sizes and free space before and after cleanup.
