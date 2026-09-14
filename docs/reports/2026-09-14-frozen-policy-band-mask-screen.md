# Frozen-policy structural band mask screen

Date: 2026-09-14. Status: complete. No training updates; all experiment workers
exited, and the main run remains paused at update 1908.

## Result

Masking structurally impossible bands improved both frozen baseline models on
the reused confirmation panel. Mean area reduction increased from **24.328% to
28.239%**, a **+3.911 pp** change. The paired, size-stratified geometry bootstrap
interval is **[+2.318, +5.492] pp**. The improvement is larger for large factories.

| Frozen model | Original | Masked | Change pp |
| --- | ---: | ---: | ---: |
| Initialization 71, update 128 | 27.580% | 31.353% | +3.773 |
| Initialization 113, update 128 | 21.075% | 25.125% | +4.050 |
| Mean of both | 24.328% | 28.239% | +3.911 |

| Size | Original | Masked | Change pp |
| --- | ---: | ---: | ---: |
| Small | 29.515% | 32.005% | +2.490 |
| Large | 19.140% | 24.473% | +5.333 |

After averaging both model seeds and four action draws, 23 of 32 geometries
improve, 7 worsen, and 2 tie. Per-model intervals are [+1.701, +5.964] pp for
seed 71 and [+2.017, +6.184] pp for seed 113. These intervals condition on the
observed models/draws; the panel was already used to diagnose the problem.
This is evidence for an execution-time intervention on these frozen policies,
not evidence that masked PPO learns faster or generalizes to a new population.

## Fixed experiment and correctness

The pre-outcome protocol is documented in
`../designs/2026-09-14-frozen-policy-band-mask-screen.md`. There are 512 measured
episodes: two frozen initialization seeds, two conditions, 32 geometries balanced
by size, and four fixed action draws each. All episodes receive 64 decisions,
10,000 ms native session allowance, and the original native route limits.
An additional 32 designated warmups are excluded from outcomes.

Both conditions retain the original candidate list/order, graph features,
per-kind count normalization and joint conditional port decoder. The masked
condition changes only logits for bands that cut a rigid body/component or
delete an original non-facility anchor. The original distribution is conditioned
on the surviving actions. Temporary overlap, unfinished routing, and all
placement actions remain available.

All 512 returned best factories passed native validation and all 512 episodes
ended at their action limit, with no resource termination. Model digests matched
in the coordinator and every worker after each block. Bound source checkpoint
files remained unchanged. Both training runs had reserved this panel's geometry
identities; no evaluation input was used to update model weights here.

The original condition reproduces the complete saved best-score tuple in all
128 episodes shared with the earlier confirmation evaluation. Counts/work also
match in all 64 seed-71 episodes and 62 of 64 seed-113 episodes. The two remaining
seed-113 episodes (719200280/sample0 and 719200808/sample1) have different
counts/work but identical saved scores. The policy already observes elapsed
time, so execution-to-execution trajectory equality is not assumed. These
differences do not change the repeated original score comparison.

The effect is positive on both the reused and additional action-draw pairs:

| Action samples | Original | Masked | Change pp |
| --- | ---: | ---: | ---: |
| 0/1, matching earlier evaluation seeds | 23.777% | 28.206% | +4.429 |
| 2/3, additional draws | 24.878% | 28.272% | +3.394 |

## Behavior and cost

The targeted rejections fall from **8,155 / 16,384 decisions (49.77%) to zero**.
Other rejections remain: masked episodes contain 290 port-coordinate rejections.

| Mean count per 64-decision episode | Original | Masked |
| --- | ---: | ---: |
| All rejected actions | 32.590 | 1.133 |
| Placement actions | 18.723 | 38.117 |
| Delete-band actions | 43.773 | 23.422 |
| Insert-band actions | 1.504 | 2.461 |
| Improved outcomes, including non-area score improvements | 8.676 | 11.125 |
| Post-action pending state | 28.777 | 44.969 |
| Post-action overlap state | 15.895 | 32.988 |

Unfinished final drafts increase from 183/256 to 235/256 episodes, while retained
validated best scores improve. This supports preserving preparation actions;
draft incompleteness should not itself be masked away. More available decisions
also produce more native repair work: route calls increase from 49.51 to 126.74
per episode and route expansions from 18,140 to 47,784.

| Integrated cost, both model blocks | Original | Masked | Change |
| --- | ---: | ---: | ---: |
| Parallel block wall time | 25.161 s | 27.358 s | +8.73% |
| Summed worker CPU time | 170.951 s | 181.085 s | +5.93% |
| Mean evaluator time per episode | 0.656 s | 0.697 s | +6.15% |

The evaluator's time includes writing its retained compressed snapshot; this is
not a network-inference-only measurement despite the inherited field name
`inference_seconds`. Mask construction plus logit application costs 12.68 ms per
episode on average, approximately 0.198 ms per decision. More native work and
different trajectories also contribute to total cost. These are single-pass
bounded measurements with persistent workers and balanced condition order,
not repeated performance benchmarks.

The guarded evaluation section, including warmup and worker lifetime, took
59.51 seconds; model/source admission and later bootstrap analysis are outside
that timer. Peak observed process-tree RSS was 17,922,912,256 bytes, below the
32 GiB limit. Retained measured snapshots occupy 288,715,553 bytes. All 16 worker
PIDs had exited at closeout. Warmup snapshots were removed after their compact
reports were retained; measured snapshots remain on the research host.

## Reproduction, artifacts, and verification

Research host root:
`/home/ashrimp/Srv/aic-band-mask-screen-20260914`. The existing frozen runtime
was imported read-only through `AIC_MASK_SOURCE_ROOT`; only new research scripts
and outputs were installed under the new root. The normal learner, PPO update,
input encoder, native engine, and completed cold-study files were not modified.

```sh
env AIC_MASK_SOURCE_ROOT=/home/ashrimp/Srv/aic-quality-cold-continue-20260914 /home/ashrimp/Srv/aic-quality-cold-continue-20260914/tools/factory-edit-pilot/.venv/bin/python tools/factory-edit-pilot/operations/band_mask_experiment.py --branch71 /home/ashrimp/Srv/aic-quality-cold-continue-20260914/artifacts/study/development-seed71-baseline-8 --branch113 /home/ashrimp/Srv/aic-quality-cold-continue-20260914/artifacts/study/confirmation-seed113-baseline-8 --panel /home/ashrimp/Srv/aic-quality-cold-start-r2-20260914/artifacts/study/confirmation-panel/panel.json --output /path/to/fresh/output
```

Run from the checkout containing the research scripts. The result file is the
authoritative completion/outcome record. The checked-in companion manifest
retains compact results, source/checkpoint/panel bindings, block reports,
control continuity, and process closeout. Full local report copies are under
ignored `artifacts/band-mask-screen-20260914/`; raw measured episode snapshots
remain under the host root's `artifacts/screen/seed-*/{original,masked}/`.

Five targeted tests passed on the host, including native evaluation of every
proposed band in the bounded fixture audit and exact unmasked sampling
equivalence. Local pure-Python tests passed with the native test skipped.
`cargo check --workspace`, `cargo test --workspace`, and worktree/index
tracked-file checks passed. The native audit was separate from measured runs.

The user subsequently authorized training original and masked policies from
random initialization. That follow-on experiment must pair initial weights and
training inputs and compare original-trained-plus-mask with masked-trained-plus-
mask to distinguish learning benefit from this execution-time effect.
