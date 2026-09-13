# Declarative rollout batch comparison

Status: complete. All four arms finished 32,768 actions each: 131,072 training
actions and 2,048 complete episodes in 4,590.95 seconds (76.52 minutes). Batch
1,024 improved the two-seed mean on the common disjoint evaluation cohort and
reduced mean elapsed time by 6.26%, while consuming more host memory. Large-factory
results differed substantially by learner seed; this is not a consistent winner
across seeds or evidence of convergence.

## Question and retained contract

Does a 1,024-transition rollout batch improve learning over 256 transitions at
the same 32,768-action training budget and minibatch 64? The fixed
[protocol](../../tools/factory-edit-pilot/declarative-batch-comparison.json) uses
paired fresh model seeds 17 and 29. The
[design](../designs/2026-09-13-declarative-batch-size-comparison.md) records the
continuity checklist and operational bounds before the expensive run.

Both configurations retain the existing generated products, rates and complete
randomized layouts. They use the declarative action contract, frozen directed
blueprint, automatic routing/validation, retained valid best, packed width-40
model with two message rounds, CPU behavior sampling and CUDA updates. Each
logical stream retains a 64-action episode over two 32-action rollout chunks.
PPO/SIL settings and the strict-area-improvement 80/10/10 start curriculum are
unchanged. No previously trained model is imported or overwritten.

Eight physical workers use eight independent logical streams for batch 256 and
thirty-two for batch 1,024. Complete logical batches are collected under one
frozen behavior model; optimizer updates occur only afterward. There are 128
versus 32 updates, respectively. Every arm executes 512 complete episodes.

A seed-specific canonical store binds each fresh global episode slot to its
actual request and initial-layout payload. The two arms reuse the same payload
when both consume that slot as fresh. Inherited starts remain policy dependent.
Order is counterbalanced: 17/256, 17/1,024, 29/1,024, 29/256. Shared input-cache
warmth can affect elapsed time; input-access worker sums overlap collection wall
time and cannot simply be subtracted from it.

The different wave sizes also change fractional bootstrap quotas, available
improved-parent pools and realized depth mix. This compares deployed batch and
collection configurations; it does not isolate gradient noise alone. Two learner
seeds do not establish convergence or arbitrary-input generalization.

## Evaluation and evidence boundary

The previously frozen 32-factory evaluation panel is unchanged. Each checkpoint
uses two sampling replicates per factory, 64 actions and a ten-second allowance.
Evaluation occurs at 0, 16,384 and 32,768 training actions in each arm. The final
report excludes the union of actual training geometries across all four arms to
form one common disjoint cohort, and also retains full-panel results. Multiple
samples of one factory are averaged before comparison.

The separate readiness process passed in 152.50 seconds: seven input/curriculum
tests, eight-stream equivalence, six actual updates per batch and restoration of
unfinished native episodes, models, Adam states, RNG and replay. An initial fixture comparison failed because real elapsed-time observations
differed between executions. The corrected fixture comparison fixes only its
policy elapsed-time observation to provide identical inputs; actual native deadlines remain enabled. Normal readiness rollouts and
main training retain real elapsed observations. Eight existing learner tests
also passed with the selected release native library. Local Rust workspace
check and tests passed. No profiling or replay audit is attached to training.

## Completed quality comparison

Five evaluation geometries overlap the union of actual training geometries:
three small and two large factories. The common disjoint cohort therefore has
13 small and 14 large factories. Exclusions are identical for both batches and
both seeds. All 768 evaluation episodes across twelve checkpoints exported valid
factories and reached the 64-action horizon. Initial paired model evaluations
match exactly for each seed. Values below average each factory's two samples,
then average the two learner seeds. Higher area reduction is better.

| Training actions | Batch | Small, disjoint 13 | Large, disjoint 14 | All disjoint 27 |
| ---: | ---: | ---: | ---: | ---: |
| 0 | Both | 4.587% | 0.246% | 2.336% |
| 16,384 | 256 | 14.172% | 1.501% | 7.602% |
| 16,384 | 1,024 | 28.778% | 4.357% | 16.115% |
| 32,768 | 256 | 31.368% | 11.026% | 20.820% |
| 32,768 | 1,024 | 35.061% | 15.359% | 24.846% |

Final differences favor 1,024 by 3.694 percentage points on small factories and
4.334 points on large factories. The all-factory column weights the 13/14
factories equally, rather than giving each size stratum equal weight.

| Seed | Batch | Final small, disjoint | Final large, disjoint | Small, full 16 | Large, full 16 |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 17 | 256 | 32.747% | 21.215% | 30.877% | 21.161% |
| 17 | 1,024 | 34.774% | 11.468% | 33.702% | 11.274% |
| 29 | 256 | 29.989% | 0.837% | 31.634% | 0.806% |
| 29 | 1,024 | 35.349% | 19.251% | 34.323% | 19.597% |

The large-factory batch difference is -9.747 points for seed 17 and +18.414 for
seed 29. Seed 29's batch-256 large score falls from 2.134% at midpoint to 0.837%
at the endpoint; its batch-1,024 score rises from 5.856% to 19.251%. These are
observed policy evaluation changes, not identified causes. The results support
1,024 as a promising short-run configuration, particularly early in training,
but do not justify a claim that larger batches always improve large factories.
The existing default configuration is unchanged.

## Runtime, memory and actual input coverage

| Seed | Batch | Arm wall, s | Collection, s | Optimizer, s | Publication, s | Input access, worker-s | Peak RSS, GiB |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 17 | 256 | 1,171.532 | 463.879 | 115.600 | 380.550 | 74.006 | 14.192 |
| 17 | 1,024 | 1,054.038 | 380.601 | 100.494 | 332.336 | 18.105 | 20.464 |
| 29 | 1,024 | 1,155.661 | 440.191 | 108.096 | 384.188 | 70.607 | 21.961 |
| 29 | 256 | 1,185.851 | 470.955 | 119.588 | 358.682 | 17.322 | 14.300 |

Mean arm wall time is 1,178.691 versus 1,104.850 seconds (19.645 versus 18.414
minutes), a 6.26% reduction for batch 1,024. Evaluation adds about 25–26 seconds
per arm. Phase timers omit some process setup, cache hydration and bookkeeping;
they are not an exhaustive decomposition. Input-access sums overlap collection
and include the strong first-arm/second-arm shared-cache difference shown above.
Do not infer an isolated optimizer speedup from wall time alone.

All runs used the same eight worker cores, coordinator allocation and CUDA
learner; observed native concurrency reached eight. Batch 1,024's host-memory
peak was 20.464–21.961 GiB versus 14.192–14.300 GiB for 256. CUDA peak allocation
was below 0.35 GiB in every arm. No registered time, disk or RSS bound was hit.
Training workers had exited before report collection. Retained per-arm outputs
were about 1.28–1.38 GiB, with shared generated-input banks stored separately.

| Seed | Batch | Requested fresh / depth 1 / depth 2 | Actual fresh / depth 1 / depth 2 | Rejected generation attempts attached to consumed fresh inputs |
| ---: | ---: | --- | --- | ---: |
| 17 | 256 | 411 / 51 / 50 | 437 / 50 / 25 | 8 |
| 17 | 1,024 | 413 / 51 / 48 | 441 / 48 / 23 | 8 |
| 29 | 1,024 | 413 / 51 / 48 | 431 / 48 / 33 | 4 |
| 29 | 256 | 411 / 51 / 50 | 426 / 51 / 35 | 3 |

All 512 episode slots per arm completed. Across both arms, seed 17 shares 378
actually consumed fresh slots and seed 29 shares 355; payload hashes and actual
input references match on every such slot. The union contains 1,002 generated
factories from 1,017 generation attempts, including 15 rejected attempts. These
are distinct generation jobs; the per-arm rejection counts above can repeat a
cached input's recorded attempts. There were no exhausted input-generation
slots and no fixed-factory fallback. All four configured products were present
in every arm. Exact product counts and per-depth improvement/fill statistics
remain in the external compact report.

Depth-one fill was 94.1–100%; depth-two fill was 47.9–70%. Fresh substitution
therefore remained material despite the nominal 80/10/10 target. Different wave
sizes and policy-dependent donors prevent a perfectly paired inherited-input
comparison. PPO minibatches remain 64; actor KL stopping is active, so actual
actor epochs differ by update. Counts and successful-replay eligibility are
retained in the report, rather than assuming four actor epochs always ran.

## Completion and retained artifacts

The integrated report command passed its action/input/evaluation coverage,
physical-validity and paired-input identity checks using existing records.
Source, native, protocol and evaluation inputs remained fixed throughout all
four arms. Normal per-update retention removed unneeded checkpoints/chunks and
midpoint raw evaluation traces while preserving selected checkpoints, recovery
states, replay source chunks and all actual inputs they require. The selected
initial, midpoint and final evaluations/checkpoints remain available; midpoint
evaluation summaries are retained even though their raw traces are disposable.

[Compact provenance](2026-09-13-declarative-batch-provenance.json) records the
server location, complete external experiment/report hashes, source binding,
readiness receipt, four selected final checkpoints and tracking URLs. The full
source manifest, actual inputs and learner state remain external. Existing
trained predecessor checkpoints were not modified. This slice adds explicit
logical-stream multiplexing and canonical input identity; it reuses the native
editor, model and optimizer without introducing a second learning algorithm.

## Reproduction

Use the selected release-native build and the retained frozen panel. The launch
binds protocol, source, native, panel and readiness hashes. Run each command from
the repository root with the established Python environment:

```sh
python tools/factory-edit-pilot/declarative_batch_experiment.py verify \
  --native-library target/release/libaic_pilot_native.so \
  --panel /absolute/path/to/panel.json \
  --output artifacts/declarative-batch/verification
python tools/factory-edit-pilot/declarative_batch_experiment.py prepare \
  --native-library target/release/libaic_pilot_native.so \
  --panel /absolute/path/to/panel.json \
  --verification artifacts/declarative-batch/verification/verification.json \
  --output artifacts/declarative-batch/comparison
python tools/factory-edit-pilot/declarative_batch_experiment.py run \
  --output artifacts/declarative-batch/comparison --environment .tracking.env
python tools/factory-edit-pilot/declarative_batch_experiment.py report \
  --output artifacts/declarative-batch/comparison
```

The report command reads retained compact records; it runs no new evaluations.
Raw checkpoints, input banks, rollout chunks and generated reports remain under
ignored `artifacts/`. They are required to preserve actual interrupted or trained
streams; a seed alone is not a restoration artifact.
