# H6-trained model with H12 search: new-seed confirmation

Status: all six planned searches complete. The mixed setup materially
improves over H6 training with H6 search, but fails the predeclared
preference gate against H12 training with H12 search. The latter remains
the selected candidate for a fresh restart with lower training restored.

## Question and method

The [same-search cross-evaluation](2026-09-26-hierarchical-h12-cross-result.md)
made H6-trained weights with H12 search look slightly better at root seed
20260922. Because that seed selected the mixed hypothesis, evaluate it under
the other two prespecified seeds, 20260923 and 20260924, on the same paired
64-case diagnostic. Keep the three H6-trained final checkpoint bytes and
lower checkpoint fixed, change only the outer model's search contract, and
compare each new report with the retained H6/H6 and H12/H12 reports under
the same seed, stream, panel and case IDs. Use that seed's H6-search starting
model success mask for retention.

The [protocol](2026-09-26-hierarchical-h12-mixed-seed-protocol.json) binds
all input, binary, model and runner hashes and the numeric gate fixed in the
[design](../designs/2026-09-25-hierarchical-recovery-research.md) before
these new outcomes. The [progress](2026-09-26-hierarchical-h12-mixed-seed-progress.json)
retains all six success masks, paired source hashes and runtimes. Runner
source commit `24d5e91b`.

## Results

Each cell is `completed / lost starting successes` out of 64 diagnostic
cases. The loss baseline is 21 starting successes at seed 20260923 and 25
at seed 20260924.

| Seed | Stream | H6 model/H6 search | H6 model/H12 search | H12 model/H12 search |
| --- | --- | ---: | ---: | ---: |
| 20260923 | s1 | 23 / 11 | 41 / 6 | 44 / 5 |
| 20260923 | s2 | 22 / 15 | 40 / 8 | 44 / 2 |
| 20260923 | s3 | 27 / 13 | 41 / 8 | 46 / 6 |
| 20260924 | s1 | 23 / 12 | 43 / 6 | 38 / 7 |
| 20260924 | s2 | 24 / 12 | 41 / 7 | 42 / 5 |
| 20260924 | s3 | 28 / 12 | 34 / 9 | 41 / 7 |

Against H6/H6, the mixed setup gains 16.7 and 14.3 completions per 64 on
average by seed, 15.5 across all six pairs, and loses 5.2 fewer starting
successes on average. All three prespecified improvement checks pass.

Against H12/H12, however, mixed completion differences average **−4** on
seed 20260923 and **−1** on seed 20260924, or **−2.5** overall. The mixed
setup loses two more starting successes on average. It fails all three
predeclared preference checks: overall difference must be at least −2,
each seed mean at least −2, and retention no worse. The [summary](2026-09-26-hierarchical-h12-mixed-seed-summary.json)
records the checks and every gained/lost case count. The six new integrated
64-case evaluations averaged 204.8 seconds each.

The root-seed reversal matters: mixed was slightly better than H12/H12 on
seed 20260922, but H12/H12 was clearly better on the next seed. The paired
H12/H12 three-seed advantage over H6/H6 remains independently confirmed.
This comparison still reuses a training-partition diagnostic and frozen
lower training. It selects the candidate for a new lower-trained on-policy
restart; it is not held-out evidence or a production change.

## Reproduction and closeout

With the retained source checkpoints and LibTorch environment under the
recorded server workspace:

```sh
python3 tools/factory-edit-pilot/hierarchical_horizon_mixed_seed.py prepare \
  --output artifacts/recovery-horizon-onpolicy-v2
python3 tools/factory-edit-pilot/hierarchical_horizon_mixed_seed.py run \
  --output artifacts/recovery-horizon-onpolicy-v2
python3 tools/factory-edit-pilot/hierarchical_horizon_mixed_seed.py summarize \
  --output artifacts/recovery-horizon-onpolicy-v2
```

The [cleanup manifest](2026-09-26-hierarchical-h12-mixed-seed-cleanup.json)
records removal of replayable raw reports and logs after compact evidence
verification. The source final checkpoints remain for the integrated
lower-training comparison and subsequent reconstruction.
