# Hierarchical outer-policy update-budget probe result

Status: closed after all 12 predeclared arms and W&B history step 12. The
[protocol](../designs/2026-09-25-hierarchical-update-budget-probe.md) was fixed
before any diagnostic outcome. This is a single-generation mechanism probe, not
a production model selection or a multi-generation correction confirmation.

## Question and method

Test whether continued fitting to one frozen MCTS collection makes the outer
policy lose previously solved cases. Three new 32-case training streams used
the same generation-87 lower and outer checkpoints. For each stream, one search
collection was reused by four independent training prefixes: 25, 50, 100 and
200 epochs, or 400, 800, 1,600 and 3,200 updates. The lower model was frozen,
failed-root policy targets remained enabled, replay was off, and all other
generation settings were fixed. All models were evaluated on the same new
64-case train-source panel, disjoint by problem identity from the three probe
training panels and both previous stability diagnostic panels. The common
start solved 36/64. The historical 128-case validation and final held-out
panel were not used.

The initial lower and outer checkpoint SHA-256 values were respectively
`353d560842da6c4270605da552a04adc8d96411512d8d3b847625cf9c502dea9`
and `6a1f61578aa29a9e80d90ab7805bfec2ef8e543bd24fd9db90441fbb5d724520`.
The new diagnostic SHA-256 was
`b76e49e443d80a7359bfe7fd1935c1d22e7b3a2dfca45fd58d86ff1c9488d6a3`.
The [prepared protocol](2026-09-25-hierarchical-update-budget-protocol.json)
records all panel, config and executable hashes. The exact Rust training source
is commit `31555f5`, the release binary SHA-256 is
`e594b5425074a03731524d8c91991126232d840ef44a86e8b0630e1f16800fdf`,
and the predeclared experiment code is commit
`4048baca7944e46ab6b9bb7dd63620f695760a74`.

## Results

`Gained/lost` counts cases newly solved or lost relative to the same 36/64
starting model. `Easy` means at most four connections. `Move` is the outer
checkpoint's L2 displacement divided by the starting model's L2 norm. Every
failure terminated as `no-macro-actions`; there were no other failure classes.

| Stream | Epochs / updates | Completed | Gained / lost | Easy | Final policy loss | Move |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| S1 | 25 / 400 | 31 | 9 / 14 | 19 | 5.664 | 2.22% |
| S1 | 50 / 800 | 27 | 8 / 17 | 16 | 4.946 | 3.31% |
| S1 | 100 / 1,600 | 32 | 11 / 15 | 14 | 4.752 | 4.86% |
| S1 | 200 / 3,200 | 29 | 8 / 15 | 16 | 5.074 | 7.09% |
| S2 | 25 / 400 | 29 | 8 / 15 | 16 | 4.524 | 2.37% |
| S2 | 50 / 800 | 27 | 12 / 21 | 13 | 4.880 | 3.35% |
| S2 | 100 / 1,600 | 33 | 13 / 16 | 15 | 5.047 | 4.72% |
| S2 | 200 / 3,200 | 31 | 8 / 13 | 17 | 4.956 | 6.71% |
| S3 | 25 / 400 | 31 | 9 / 14 | 15 | 5.681 | 2.37% |
| S3 | 50 / 800 | 29 | 8 / 15 | 15 | 5.252 | 3.40% |
| S3 | 100 / 1,600 | 29 | 10 / 17 | 15 | 4.893 | 4.86% |
| S3 | 200 / 3,200 | 27 | 11 / 20 | 14 | 4.469 | 6.69% |

The 100-epoch arm completed 3, 2 and 2 more cases than 200 epochs in S1,
S2 and S3. However, its **retention advantage** (how many fewer starting-model
successes it lost) was 0, -3 and +3. Its mean advantage was zero and only one
stream had a positive advantage. The predeclared overshoot signal required at
least three fewer losses on average, positive retention in at least two streams,
and no more than two fewer completions on average. It **failed** the first two
conditions; the completion condition passed. In the direct 100-versus-200
pairing, 100 epochs alone solved 15/13/16 cases, while 200 epochs alone solved
12/11/14 cases across S1/S2/S3. Large bidirectional case churn persists even
where aggregate completion improves.

The outcome is not a smooth performance-versus-update curve. All three
streams fell from 25 to 50 epochs; S1 and S2 then rebounded at 100 epochs.
Weight displacement increased with the budget, but completion and loss did
not track it monotonically. The experiment supports sensitivity to additional
outer-policy updates and large search-path turnover, while it does not
establish that a shorter budget consistently preserves solved cases. It cannot
distinguish uncertain policy targets from the search's discontinuous response
to small policy changes. No update budget or checkpoint is promoted, and lower
training or a long curriculum run is not resumed on this evidence.

The [compact summary](2026-09-25-hierarchical-update-budget-summary.json)
contains all 12 result rows, termination counts, model movement, pairwise
contrasts, and the gate calculation. The [case-level progress](2026-09-25-hierarchical-update-budget-progress.json)
retains every 64-bit success mask and artifact hash; the
[baseline](2026-09-25-hierarchical-update-budget-baseline.json) retains the
common-start mask. They permit independent recalculation without raw search
traces. The [W&B observer record](2026-09-25-hierarchical-update-budget-wandb-observer.json)
shows remote acknowledgement through step 12 in the
[run](https://wandb.ai/acrylicshrimp-personal/endfield-aic-layout-gen/runs/15e620e291a72bc63ba30b8e).

## Runtime, reconstruction, and operational notes

The integrated supervised run took 2,788 seconds (46.5 minutes), including
search, training, validation, publication and tracking. The three frozen
collections took 116, 103 and 130 seconds of search; the 64-case baseline took
199 seconds. All 12 training runs together took about 64 seconds. The 13
diagnostic search reports account for most wall time and storage. The server
artifact tree occupied 3,354,921,162 bytes before cleanup. All final lower
weight hashes matched the starting lower checkpoint, and every output matched
its declared collection, update count and configuration hash.

After the compact package was verified readable, the replayable experiment
artifact tree and this checkout's release build cache were removed. The
[cleanup manifest](2026-09-25-hierarchical-update-budget-cleanup.json) records
the exact paths and sizes; server free space increased by 4,065,914,880 bytes.

To reproduce in a fresh server artifact root under `~/Srv`, use the tracked
preparation script, then the supervised matrix and summarizer:

```bash
cd ~/Srv/aic-policy-stability-20260923
bash tools/factory-edit-pilot/prepare_hierarchical_update_budget.sh
bash tools/factory-edit-pilot/run_hierarchical_update_budget_probe.sh \
  "$HOME/Srv/aic-gumbel-root-20260922/source/.env" \
  4048baca7944e46ab6b9bb7dd63620f695760a74
~/Srv/aic-rust-learning-20260915/.venv/bin/python \
  tools/factory-edit-pilot/summarize_hierarchical_update_budget.py \
  --output artifacts/update-budget-probe
```

The initial supervisor had a narrow readiness-state race: W&B had acknowledged
step 0, but the observer moved from `ready` to `running` before the supervisor
read it. After verifying remote step 0, the status was bridged once and the
supervisor condition was fixed in commit `5fa73db` to accept both states. A
live copy of that shell fix caused a parser error after the supervisor had
printed `matrix_complete=12 remote_confirmed_step=12`; the matrix, final
observer, and independent summarizer all completed and passed their checks.
No training or evaluation code, input, result, or threshold changed during the
run. The fixed supervisor passed `bash -n` and is the version to use for a
fresh reproduction.

## Decision

Do not select 100 epochs solely from its three aggregate wins. The next
bounded diagnosis should hold one trained checkpoint fixed and inspect whether
root policy changes flip the first macro action on the 100-versus-200 discordant
cases, then test target quality or search sensitivity with a predeclared paired
check. Any proposed correction still needs multi-generation retention and an
untouched final held-out evaluation before integrated training.
