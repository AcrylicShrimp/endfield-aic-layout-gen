# Hierarchical search budget diagnosis

Status: accepted diagnostic contract

## Question and stopping point

Can additional MCTS simulations, without changing checkpoints, action
generation, progressive widening, or training, recover the fixed curriculum
regression panel efficiently enough to justify a larger collection budget?

The falsifiable budget-only hypothesis is that one tested budget reaches at
least 27/32 completions, which is 90% of the 30/32 generation-nine-lower plus
anchor-prior control, without a disproportionate wall-time increase. The
diagnosis stops after the fixed matrix and reports either the smallest efficient
qualifying budget or that budget scaling alone is not an adequate treatment. It
does not implement a new exploration algorithm or resume curriculum training.

## Controlled formulation

Use source commit `5e8cbf4d6d45425ce916b597fc1961400868c292`, the retained
generation-nine lower and outer checkpoints, normalized catalogs, CUDA and the
tracked 32-case policy-collapse panel. The panel contains eight easy controls,
17 early hard successes and seven early hard failures, all drawn from training
collections. It contains no held-out cases.

Keep every canonical search field fixed except `simulations_per_move`. Run these
budgets in ascending order:

```text
64, 128, 256, 512, 1024, 2048
```

Each cell receives the same panel, case order, checkpoints, horizon, move limit,
action candidates, workers, exploration constant, anchor-prior mass and device.
An incomplete or interrupted cell is `unknown`, not a failure or infeasibility
result. Do not train or modify either checkpoint.

## Measurements

Retain a compact machine-readable summary with, for every budget:

- completed cases overall and by panel group;
- termination counts and per-case outcome;
- wall-clock seconds and MCTS samples per minute;
- speed relative to the 64-simulation baseline;
- marginal completions and marginal seconds from the preceding budget;
- completion-seconds and simulations per completion;
- report, configuration, panel and checkpoint SHA-256 identities.

Calculate the deterministic root eligibility of the 21 actions that the anchor
control used to recover learned-policy failures. For rank `r`, the present
progressive-widening rule first admits the action after
`(r - 1)^2 + 1` parent visits. Report eligible counts for every tested budget.
This is a root coverage certificate, not an end-to-end success probability.

Use report root records to summarize selected prior rank, visit concentration
and how often the final selection equals the learned prior top action. These
measure whether additional simulations actually overcome the prior after an
action becomes eligible.

## Decision rule

First construct the Pareto frontier over completion and wall time. A qualifying
budget must complete at least 27/32 cases. Among qualifying cells, select the
smallest budget whose completion is within one case of the maximum observed
completion. Reject a larger budget when it adds no completion or when its first
additional completion costs more wall time than rerunning the preceding cell
once.

If no budget qualifies, report the knee only as the most efficient diagnostic
budget and reject simulation scaling as the production fix. The next research
decision may compare a bounded Gumbel/anchor candidate allocator against that
knee, but that is outside this diagnostic slice and requires a separate
contract.

## Retention and verification

Run the release binary on `ashrimp-arch` after checking disk, GPU and active
processes. Retain the exact configurations, commands, compact summary and report
hashes under a task-specific `artifacts/` directory. Raw per-action reports may
be deleted after their hashes and required measurements are verified. Validate
the repository documentation and source-asset registry before committing the
completed diagnosis.
