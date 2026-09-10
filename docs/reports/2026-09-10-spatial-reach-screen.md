# Actor spatial receptive-field screen

Keep the two-layer actor as the reference under this 32-batch budget. It has the
highest final score and largest gain from its own untrained policy. The dilated
four-layer actor beats the ordinary four-layer actor on 19 of 32 factories, but
neither 17 by 17 actor beats the two-layer control. This screen supports a modest
benefit from dilation within the four-layer architecture; it does not establish
that eight layers are necessary or that CNN reach is the dominant current limit.

All four runs completed exactly 32 batches and passed final native, checkpoint
and online tracking audits. No further training was started.

## Paired quality results

The metric is mean per-factory percent reduction of best used area from the
identical initial factory. Higher is better. Every evaluation checkpoint covers
the same 32 factories in all four runs.

| Actor | Untrained | Batch 8 | Batch 16 | Batch 24 | Batch 32 | Gain from untrained |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Two layers, 5 by 5 | 47.33% | 50.70% | 52.11% | 54.01% | **56.62%** | **+9.29 pp** |
| Four layers, 9 by 9 | 49.03% | 48.73% | 50.95% | 53.11% | 54.23% | +5.19 pp |
| Four dilated layers, 17 by 17 | 48.63% | 48.42% | 51.67% | 53.65% | 55.27% | +6.65 pp |
| Eight layers, 17 by 17 | 46.85% | 50.15% | 51.17% | 53.37% | 54.04% | +7.19 pp |

At batch 32, ordinary four layers win/tie/lose 4/7/21 factories against two
layers; dilated four layers do so on 9/6/17; eight layers on 6/3/23. Their mean
differences from two layers are -2.39, -1.35 and -2.58 percentage points.

The predeclared four-layer contrast favors dilation by 1.05 pp, with 19 wins,
five ties and eight losses. Its improvement from its own initialization exceeds
the ordinary four-layer improvement by 1.45 pp. These models have identical
initial parameter tensors as well as parameter counts. Their untrained behaviors
can still differ because dilation changes the computation.

At equal nominal 17 by 17 reach, eight layers trail dilated four layers by
1.23 pp, with 11 wins, five ties and 16 losses. Eight layers start 1.78 pp lower
and recover more (+7.19 versus +6.65 pp), so their lower final score alone should
not be called a worse learning slope. The two-layer control nevertheless leads
both final quality and learning delta in this run.

These results do not justify promoting eight layers. They also do not prove
that five by five local context is universally sufficient. Different depth and
residual paths change optimization, and the fixed PPO setting produces different
numbers of optimizer steps, documented below.

## Where the gains occur

Final direct contributions sum to the measured best-area reduction:

| Actor | Empty-strip contraction | Single-facility pose | Group pose |
| --- | ---: | ---: | ---: |
| Two layers | 50.48 pp | 4.76 pp | 1.38 pp |
| Four ordinary layers | 49.02 pp | 3.80 pp | 1.40 pp |
| Four dilated layers | 49.26 pp | 4.05 pp | 1.97 pp |
| Eight layers | 49.86 pp | 3.39 pp | 0.79 pp |

Other action kinds have zero direct best-area gain at final evaluation. They can
still precede later gains. The separate future-credit plot retains reward-to-go
for every selected action and subtracts the mean at the same action index; it
must not be read as causal attribution. The four-layer dilation advantage splits
into about +0.24 pp contraction, +0.25 pp single poses and +0.56 pp group poses.

All selected final-evaluation poses have nonempty initial route-chain regions.
Among these selected poses, native validation succeeds 200/323, 168/295, 165/260
and 145/237 times in experiment order. Single 5 by 5 poses validate 125/188,
87/134, 95/128 and 81/129 times. Dilation has a higher conditional 5 by 5 validity
rate than the ordinary four-layer control, while its total single-pose area gain
remains below the two-layer result. This is consistent with changed selection and
usefulness, not proof of improved geometric classification.

Selected single 3 by 3 and 4 by 6 moves produce no immediate best-area gains in
these final trajectories. They have different counts and can have indirect value;
this does not establish that those facility types are unimportant. Group moves
may include mixed facility sizes.

| Final mean reduction by facility count | Two layers | Four ordinary | Four dilated | Eight layers |
| --- | ---: | ---: | ---: | ---: |
| 1-4 facilities, 15 factories | 59.31% | 56.57% | 58.77% | 56.86% |
| 5-16 facilities, two factories | 55.91% | 54.28% | 61.59% | 56.46% |
| At least 17 facilities, 15 factories | 54.02% | 51.87% | 50.94% | 50.90% |

The wider actors do not consistently improve the larger-factory stratum.
Dilation's overall advantage over ordinary four layers comes from the smaller
and middle strata; the middle stratum contains only two factories. Relative to
two layers, dilated four layers gain 1.15 pp on medical factories but lose
4.29 pp on food factories. Complete product and footprint breakdowns are retained
in the generated comparison JSON.

Across all selected action kinds, rejection diagnostics also depend on policy
selection. For example,
`local-no-path` occurs 138/118/91/124 times and `local-work-budget` occurs
73/64/37/53 times in experiment order. The dilated actor selects fewer poses than
the two-layer actor. Diagnostic occurrences can overlap within one failed action;
these counts are neither disjoint failures nor a same-action routing benchmark.

## Training work and resource use

All variants select 32,256 training actions. Sixteen of 1,024 attempted starts
retain their original admission rejection in every run. Total fresh training is
128 batches and 129,024 actions. Every accepted episode completes all 32 actions.

| Actor | Episode worker seconds | PPO update seconds | Preparation seconds within updates | KL-stopped batches | Total epochs | Optimizer steps |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Two layers | 507.66 | 522.09 | 73.53 | 7 | 117 | 1,860 |
| Four ordinary layers | 532.01 | 507.12 | 65.75 | 14 | 103 | 1,638 |
| Four dilated layers | 534.48 | 499.20 | 64.12 | 17 | 103 | 1,639 |
| Eight layers | 541.25 | 432.89 | 47.06 | 26 | 76 | 1,210 |

PPO stops an update after its sampled KL exceeds the common 0.02 threshold.
Eight layers perform only 1,210 optimizer steps versus 1,860 for two layers.
Their shorter update time accompanies substantially less optimization work;
it is not a controlled inference-speed comparison. Collection worker time rises
6.6% for eight layers. Total run durations are 690.49, 680.74, 672.63 and
606.16 seconds, including evaluation and coordinator overhead. A same-PPO,
same-batch screen measures this combined architecture/optimization behavior;
it does not isolate what each model could learn with separately tuned updates.

Peak sampled process-tree RSS is 15.34/15.37/15.39/15.48 GiB, and peak sampled GPU
usage is 4.83/4.46/4.62/4.94 GiB. No resource limit or numerical failure ended a
run. Actual inference is profiled separately below; summed episode worker time
also includes observation, sampling and native execution.

## Final audit

All 1,056 initial-source identities match across the four runs, including their
admission outcomes. All controls and horizons match. Four critics share the same
initial weights, and the two four-layer actors share their initial tensors.

Final audits verified 132 distinct checkpoint files, restored all four final
checkpoints, validated 640 evaluation results and their reward sequences, checked
all frozen runtime source hashes, and matched 5,740 W&B scalar values over 168
records. All four remote runs are `finished`, with `training_complete=true`,
`termination_reason=schedule-complete` and supervisor child exit codes `[0,0]`.
The earlier readiness checks separately establish exact optimizer/RNG continuation.

The final learning, action-outcome and cost figures were visually inspected.
Human findings and compact provenance are tracked; raw trajectories, figures and
non-replayable checkpoints remain in ignored artifact storage.

## Experiment contract

This executes `docs/designs/2026-09-10.16-spatial-receptive-field-review.md`.
Change only the actor's spatial encoder. Keep graph depth two, width 32, candidate
sampling locations, full action domain, current/history visibility, PPO reward,
native region executor and the independent critic architecture fixed.

| Actor | CNN dilations | Local convolution field | Actor parameters | Critic parameters |
| --- | --- | --- | ---: | ---: |
| Two layers | 1,1 | 5 by 5 | 50,977 | 32,961 |
| Four layers | 1,1,1,1 | 9 by 9 | 69,473 | 32,961 |
| Four dilated layers | 1,1,2,4 | 17 by 17 | 69,473 | 32,961 |
| Eight layers | Eight ones | 17 by 17 | 106,465 | 32,961 |

All variants retain the original dense two-layer stem and its one GroupNorm.
Each added layer computes `x + SiLU(Conv3x3_dilation(x))`; no extra normalization,
downsampling, changed sampling locations or graph attention is introduced.
The four-layer controls have identical parameter counts, residual structure and
initial parameter values. Dilation changes computation connectivity as well as
nominal reach, so this remains an architectural comparison rather than perfect
isolation of an abstract receptive-field scalar. Global summaries and GroupNorm
statistics still provide nonlocal aggregate dependence in every variant.

Every run starts fresh at actor seed 17. Critics initialize in an independent CPU
RNG context using seed 100017, then train normally on their own actor's data.
Their common initial parameter hash is
`4720ed9a6b50d94983168ddc31f75557aef8461fde702813f915d98efb66112c`.
The original region pilot's 57.99% two-layer result used another initial critic;
it is historical context, not this experiment's matched control. The actor-only
untrained two-layer behavior remains the same.

The versioned model specification records CNN layers/dilations separately from
graph layers. Immutable protocol hashes bind the specification, source files,
inputs and binary identities to checkpoints; strict loading never transfers a
checkpoint across treatments. Single-state collection and batched learning now
call one shared raster-forward method instead of maintaining two CNN executions.

## Data and schedule

Each run uses the same 1,056 frozen starts: 1,024 training attempts and 32 repeated
evaluation factories. Preserve original admission rejections rather than drawing
replacement factories. Train exactly 32 batches with 32 attempts and horizon 32,
then stop normally. Evaluate at 0/8/16/24/32. Collection uses eight CPU workers;
learning uses CUDA FP32 on the same RTX 5090. Route work is 5,000 per edit and all
PPO settings match the preceding region pilot. Run order is two layers, four
ordinary layers, four dilated layers, then eight layers, without concurrent GPU
training jobs.

The evaluation set contains ten food, nine copper-liquid, seven medical and six
watered-grass factories. Fifteen contain 1-4 facilities, two contain 5-16 and
fifteen contain at least 17. Their initial placements comprise 166 footprints of
3 by 3, 135 of 5 by 5 and 53 of 4 by 6, counting rotations together. These describe
this selected dataset, not catalog frequencies or universal layout limits.

Compare mean per-factory best-area reduction, paired differences from the new
two-layer control and improvement from each model's own untrained policy. Also
report full 32-action trajectories, per-product/facility-count outcomes, direct
single/group pose gains, selected-pose footprint categories, route-chain cases,
rejection diagnostic occurrences, timing, sampled memory and PPO KL behavior.
Direct contributions sum to final outcome. Future reward-to-go and validation
rates condition on actions each policy selected; they do not establish causality
or directly measure perception accuracy.

## Readiness and measured scale

All 53 PPO Python tests passed locally, including the new forward/gradient,
probability mask, independent-world, chunk, remapped-node permutation and shared
critic initialization checks. Ten input-contract tests and one precision probe
test also passed. Workspace check and tests passed: 548 Rust tests, five ignored.
No Rust behavior changed in this slice.

The three spatial test methods also passed on CUDA. Each of four separate
readiness runs completed two batches with eight attempts, horizon 32 and four
initial/final evaluation cases. Native audits checked 24 results per variant,
collector/learner behavior probabilities and values, and exact continuation of a
saved update with nonempty Adam state, including weights, optimizer states, RNG
and epoch diagnostics. Readiness weights are not reused by the comparison runs.

Before training, profile 992 retained states from the preceding region run's
batch 32. Select the median, p95 and maximum separately for raster cells, graph
nodes and actions; check complete CPU/single-state versus CUDA/batched logits and
finite gradients for each variant.

| Per-state measure | Minimum | Median | p95 | Maximum |
| --- | ---: | ---: | ---: | ---: |
| Raster cells | 84 | 209 | 2,993 | 8,134 |
| Graph nodes | 53 | 108 | 783 | 1,236 |
| Candidate actions | 38 | 206 | 2,732 | 4,373 |

The profile selects median index `n // 2` and p95 index `floor(.95 * (n - 1))`.
These are measured state sizes, not candidate caps or canvas bounds.

On the largest raster state, CPU actor forward took 6.77/7.64/8.41/11.83 ms in
experiment order. Maximum live CUDA allocations over the selected single-state
checks were approximately 170.5/174.6/174.6/182.7 MiB. These first-pass diagnostics
establish executable scale and numerical agreement; they are not warmed latency
benchmarks. Actual training batch costs and sampled process/device memory must
be compared separately.

## Reproduction and evidence

Online runs: [two layers](https://wandb.ai/acrylicshrimp-personal/endfield-aic-layout-gen/runs/ade998526af39635dbabdb3f), [four ordinary layers](https://wandb.ai/acrylicshrimp-personal/endfield-aic-layout-gen/runs/f950209ac13364e3aab1f63f), [four dilated layers](https://wandb.ai/acrylicshrimp-personal/endfield-aic-layout-gen/runs/995431e980028c202f781f87), [eight layers](https://wandb.ai/acrylicshrimp-personal/endfield-aic-layout-gen/runs/2ffe373877f0485d19cae72f).

The frozen runtime is `/home/ashrimp/Srv/aic-ppo-spatial-20260910`, using source
`951760e` plus tracking change `5d55963`. It reuses the existing locked Python
3.12 environment and copies the unchanged region native library, constructor CLI
and witness probe. Native Rust sources and game-data tables are unchanged. The
input manifest at `artifacts/factory-edit-pilot/region-starts/manifest.json` points
to the preserved matching source factories. Earlier runtimes remain unchanged.

```sh
python tools/factory-edit-pilot/profile_ppo_spatial.py \
  --run "$REGION_RUN" --output "$EVIDENCE/spatial-profile.json"
# Run the independent two-batch readiness configurations and audit first.
for variant in cnn2 cnn4 cnn4-dilated cnn8; do
  python tools/factory-edit-pilot/ppo_harness.py --env-file "$ENV_FILE" prepare \
    --config "tools/factory-edit-pilot/ppo-spatial-$variant.json" \
    --output "$RUNS_ROOT/ppo-spatial-$variant"
  python tools/factory-edit-pilot/ppo_harness.py --env-file "$ENV_FILE" run \
    --run "$RUNS_ROOT/ppo-spatial-$variant"
  python tools/factory-edit-pilot/audit_region_pilot.py \
    --run "$RUNS_ROOT/ppo-spatial-$variant" \
    --output "$EVIDENCE/$variant-final-audit" --env-file "$ENV_FILE"
done
python tools/factory-edit-pilot/compare_ppo_spatial.py \
  --runs-root "$RUNS_ROOT" --output "$EVIDENCE/comparison.json"
python tools/factory-edit-pilot/plot_ppo_spatial.py \
  --summary "$EVIDENCE/comparison.json" --output "$EVIDENCE/view"
```

Keep the runtime, initial inputs, immutable records, trained checkpoints, all
selected evaluation traces and training batches 1/2/8/16/24/32 outside Git under
ignored artifacts. Other training intermediates use the existing automatic
retention cleanup. Commit only the source/configuration, human findings and a
compact provenance manifest. Comparison and plotting CLIs passed a four-readiness
smoke check with 20 matched unique starts; generated plots were inspected.

## Interpretation limits

One initialization seed and a reused evaluation set provide architecture-selection
evidence. They do not establish generalization, seed robustness, eventual model
capacity or a universal minimum CNN depth. Changing critic initialization makes
the previous region baseline an additional reference only. A subsequent seed
replication or longer run would be a separate experiment, not an automatic
extension of this 32-batch screen.
