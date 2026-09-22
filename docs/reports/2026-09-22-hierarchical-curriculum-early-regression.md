# Hierarchical curriculum early regression

## Question and verdict

The canonical hierarchical curriculum run asked whether repeated fresh-graph
MCTS distillation would raise completion at the first one-facility curriculum
level before progressing to larger factories. The run was stopped after 11
committed generations. **Verdict: do not continue this run unchanged.** Training
and held-out completion regressed at the same curriculum level, and the decline
concentrated in the multi-connection cases that the curriculum needed to learn.

This was not a curriculum-level transition. Every committed generation remained
at level zero: one facility, up to four terminals and up to four connections.
The stopped run remains recoverable for diagnosis; generation 11 has a frozen
collection and an atomic epoch checkpoint under its pending training directory.

## Identity and method

The run used source commit
`5e8cbf4d6d45425ce916b597fc1961400868c292`, release binary SHA-256
`065f2187dae0cc951daa8d7fb315e55aef6e9c080a510fd2afd5784d86f754e7`,
and the tracked configurations under
[`hierarchical-curriculum`](../benchmarks/hierarchical-curriculum/). Its input
binding was
`653740f9371a4ac05b681b3a8881ceffb6be6b3c7bc4834dd939aa6242d8a3a9`.
The exact launch command and all input and checkpoint hashes are retained in
`artifacts/hierarchical-curriculum-v1/launch.json` on the training server. The
reproduction command is also in the
[`readiness report`](2026-09-22-hierarchical-curriculum-readiness.md).

Each generation collected MCTS records from 32 newly sampled graphs, trained
the lower and outer models for 200 epochs on that generation's records, and
promoted the resulting checkpoints atomically. The recent-success statistic
covered 128 cases, but learner updates consumed only the current generation's
records. The fixed 128-case held-out panel ran after generations 4 and 9 and did
not affect training or promotion.

## Results

Per-generation training completion was:

```text
generation     0      1      2      3      4      5      6      7      8      9     10
completed     16     16     16     17     18     15     12     11     14     12     12
rate       .5000  .5000  .5000  .5313  .5625  .4688  .3750  .3438  .4375  .3750  .3750
```

The 128-case rolling completion rate peaked at `0.5234375` after generation 4
and fell to `0.3828125` after generation 9, remaining there after generation
10. It never approached the `0.75` promotion threshold.

Fixed held-out completion declined monotonically:

| Checkpoint | Completed | Rate |
| --- | ---: | ---: |
| Pre-run warm smoke | 16/128 | 12.50% |
| After generation 4 | 11/128 | 8.59% |
| After generation 9 | 8/128 | 6.25% |

The changing panel mix explains some per-generation noise but not the failure to
learn the hard class. Across generations 0 through 9, two-connection cases
completed 127/132 times. Three-connection cases completed only 18/176 times and
four-connection cases 2/12 times. More specifically, multi-connection completion
fell from 17/91 in generations 0 through 4 to 3/97 in generations 5 through 9,
while two-connection completion remained 61/63 in the latter half. Incomplete
cases terminated as `no-macro-actions`; they were bounded failures rather than
infeasibility proofs.

The outer update also ended above its initial loss in generations 5, 7, 8 and
9. This alone is not a monotonicity contract because the report records batch
endpoints, but it agrees with the behavioral regression. W&B acknowledged all
committed steps 0 through 10 at
[run 332c36231b222cc0fbcdea3b](https://wandb.ai/acrylicshrimp-personal/endfield-aic-layout-gen/runs/332c36231b222cc0fbcdea3b).

## Interpretation and diagnosis

The first three exact `0.5` rates were observed 16/32 outcomes, not a configured
or predicted initial probability. The initial learner almost always solved the
two-connection shape and usually failed the three-or-more-connection shape;
fresh panels happened to contain roughly equal amounts of those classes.

The retained-checkpoint diagnosis is complete. Crossed checkpoint evaluations
and policy/value ablations isolate the regression to the outer policy prior;
the generation-nine lower model improves the warm outer checkpoint, and
removing the learned outer policy restores 30/32 completions. The learned prior
became concentrated enough that prior-ranked progressive widening excluded
useful actions from the search. Failed episodes then supplied the same biased
visit distributions as unqualified policy targets, while each 200-epoch update
replaced the preceding generation rather than replaying successful hard roots.
See the
[`policy-collapse diagnosis`](2026-09-22-hierarchical-curriculum-policy-collapse-diagnosis.md)
for the controlled matrix, ablations and next-run requirements.

Do not resume the original run. Before another long run, require a bounded
replay window, outcome-aware treatment of failed-root policy targets, and
anchor-guaranteed exploration to pass the fixed diagnostic panel. Keep the
held-out panel untouched for the final generalization measurement.

## Stop and retention state

Training stopped at `2026-09-22T21:38:01+09:00`. Eleven generations are fully
committed, W&B received their summaries, and generation 11 retains a resumable
pending checkpoint. The server artifact occupies 159,966,448 bytes. It remains
retained only for the stated checkpoint diagnosis and possible controlled
resume; it should be packaged and removed when that decision is complete.
