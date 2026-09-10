# Best-state curriculum readiness at 64 factories per batch

The 80/10/10 best-state start curriculum passed native and CUDA readiness with
64 attempted factories and 32 actions per factory. All trajectories are sampled
under the current batch policy. Only factory starts, complete request obligations
and ancestry are inherited. The four-batch test is a correctness/capacity check,
not evidence that the curriculum improves eventual policy quality.

The [design contract](../designs/2026-09-10-ppo-best-state-start-curriculum.md)
defines quotas, complete-pass depth, deterministic uniform parent selection,
bootstrap/shortage handling, native re-admission and retained carry-forward
bundles. Selected parent best states remain available after ordinary training
output cleanup. Training/evaluation seed separation and standard fixed evaluation
factories are preserved.

## Observed groups and admission

| Batch | Fresh attempts | One-pass attempts | Two-pass attempts | Admitted actions |
| --- | ---: | ---: | ---: | ---: |
| 1 | 64 | 0 | 0 | 2,048 |
| 2 | 58 | 6 | 0 | 2,016 |
| 3 | 51 | 7 | 6 | 1,952 |
| 4 | 51 | 6 | 7 | 1,984 |

There were 250 complete training episodes, six expected fresh-generation
rejections, and 64 complete evaluation episodes. All 19 one-pass and 13 two-pass
starts were admitted. Bootstrap substitutions were explicit; no inherited
admission failed and no post-bootstrap donor shortage occurred. Mature five-batch
windows allocate exactly 256/32/32 attempted factories; admitted proportions can
differ when fresh generation rejects an input.

The six one-pass starts in batch 2 were already 43.67% smaller than their original
starts on average and achieved another 1.92% reduction relative to their new
episode starts. The two-pass groups achieved 1.04% additional reduction in batch 3
and 4.16% in batch 4. These small correlated groups verify that inherited states
can produce fresh improvement trajectories; they are not comparative learning
results.

## Independent verification

The readiness audit checked all 320 attempted results, including native validity
of all 314 complete initial/best factories and the recorded six generation
rejections. It verified current actor/critic identities, episode reward
conservation, experience/trace correspondence and the fixed evaluation cohort.

For batch 3, which includes two-pass starts, it restored the nonempty actor/critic
Adam checkpoint from batch 2. A two-pass episode's behavior probabilities and
critic values matched recomputation. Repeating the complete third PPO update
produced identical actor, critic, optimizer, shuffle-RNG hashes and epoch
statistics. The bounded audit completed in 96.29 seconds, below its 180-second
limit.

The separate lineage audit verified consecutive parent depths, root identities,
parent best geometry and metrics, original-obligation hashes, deterministic
start-plan reconstruction and exclusion of evaluation factories from inheritance.
All 32 inherited inputs passed these checks.

The initial actor/critic tensors and CPU/CUDA/shuffle RNG states exactly match the
original untrained seed-17 CNN-2 checkpoint. Both Adam states start empty. Initial
evaluation also matches the preceding routing experiment: 47.1164% mean area
reduction, 142 failed actions and eight routing-budget failures out of 1,024.

All 67 `test_ppo*.py` tests passed, including seven curriculum tests. Tests cover
quota conservation for 32/64/128 factories, bootstrap and genuine donor shortages,
parent selection independence from scores/arrival order/global RNG, the depth
ceiling, cleanup survival, deterministic logical-task resume, corrupt/missing
inputs, native admission metric mismatch and per-depth reward denominators.
`cargo check --workspace`, `cargo test --workspace`, and tracked-file/index checks
also passed. No Rust or production planner behavior changed.

## Capacity and measurements

The complete readiness run took 230.12 seconds. Training phases took
57.41/51.74/50.92/54.62 seconds. Peak measured process-tree RSS was 16,397,836,288
bytes, below the configured 32-GiB limit. The previous experiment was running
concurrently, so these timings are not an isolated batch-size scaling benchmark.

Episode/phase records now distinguish strict area improvements from secondary
score improvements. They retain direct area contribution by action kind, first
eight versus later-action contributions, accepted physical repeats and total
physical repeats. Training additionally reports these metrics by start depth,
with separate local additional-area and root-relative cumulative-area measures.
W&B exports the same numeric metrics in the long run.

The long-run configuration uses 64 factories, 32 actions, eight CPU collectors,
CUDA FP32, unchanged PPO minibatch/microbatch settings, and the existing 0.001
failure penalty. Evaluate the same fixed 32 factories every four batches. The
first 16 new batches cover the same 1,024 scheduled fresh-input seeds as the
previous 32-factory run's first 32 batches; inherited slots intentionally replace
some fresh inputs. Batch size and the curriculum change together, so comparisons
do not isolate their individual effects.

## Reproduction

In the frozen server checkout recorded in the adjacent provenance manifest:

```sh
tools/factory-edit-pilot/.venv/bin/python tools/factory-edit-pilot/ppo_harness.py prepare \
  --config tools/factory-edit-pilot/ppo-cnn2-curriculum-readiness.json \
  --output artifacts/factory-edit-pilot/curriculum-readiness-reproduction
tools/factory-edit-pilot/.venv/bin/python tools/factory-edit-pilot/ppo_harness.py run \
  --run artifacts/factory-edit-pilot/curriculum-readiness-reproduction --duration-seconds 900
tools/factory-edit-pilot/.venv/bin/python tools/factory-edit-pilot/check_ppo_harness_native.py \
  --run artifacts/factory-edit-pilot/curriculum-readiness-reproduction \
  --output artifacts/factory-edit-pilot/curriculum-readiness-reproduction-audit
```

Readiness tracking is disabled. The replacement twelve-hour run uses a new online
W&B identity and fresh model initialization, with its launch recorded separately.
