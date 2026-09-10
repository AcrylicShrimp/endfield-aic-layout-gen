# Transactional route regions: 32-batch pilot

The treatment is promising enough for another bounded experiment: at batch 32 it
improves mean held-out best-area reduction from 53.46% to 57.99%, a 4.53 percentage
point advantage on 32 identical factories. It wins 20 cases, ties five and loses
seven. This is evidence for the combined executor treatment, not proof that
automatic surrounding-chain expansion caused the improvement.

## Paired results

All 1,056 initial-source identities matched. Each run selected 32,256 training
actions over exactly 32 batches; 16 of the 1,024 generation attempts retain their
original admission rejection. All 32 evaluation factories completed at every
checkpoint. Treatment finished normally in 671.41 seconds, with no automatic
training extension.

| Training batch | Previous reduction | Region reduction | Region minus previous |
| ---: | ---: | ---: | ---: |
| 0 | 50.19% | 47.33% | -2.87 pp |
| 8 | 50.17% | 49.14% | -1.03 pp |
| 16 | 52.87% | 52.70% | -0.17 pp |
| 24 | 52.39% | 56.23% | +3.84 pp |
| 32 | 53.46% | 57.99% | +4.53 pp |

Relative to each executor's own untrained policy, the improvement is +10.66 pp
for regions and +3.26 pp for the previous executor. Matched 32-action trajectories
are in the generated `region-pilot.png` figure. The treatment recovers from a
lower initial score and leads at both final measured checkpoints.

At batch 32, selected distant poses validate 82/138 times (59.4%) versus 31/74
(41.9%); group poses validate 57/120 (47.5%) versus 30/76 (39.5%). These are
observed rates among different selected actions, not a same-action causal test.
All-action validation actually decreases slightly, 829/1024 versus 833/1024.
The useful signal is better outcome and broader successful pose use, not uniform
success across action kinds.

The region executor retries 135 selected edits at final evaluation; eight
validate and two directly improve best area. Of 32 edits that expand into
surrounding chains, three validate and none directly improve best area. Port
retry occurs in 49 edits, three validate and one directly improves best area.
These categories overlap. First-attempt routing order/crossing changes remain
important, while the separate benefit of chain expansion is unresolved.

| Cost over 32 training batches | Previous | Region |
| --- | ---: | ---: |
| Summed episode worker seconds | 469.88 | 527.47 |
| PPO update seconds | 481.63 | 499.73 |
| Summed generation/admission seconds | 181.82 | 11.89 |
| Summed training phase seconds | 643.26 | 633.70 |

Episode worker time rises 12.3% and update time 3.8%. At final evaluation native
executor mean/p95 rises from 0.289/1.110 ms to 0.442/2.139 ms. These native timings
exclude the subsequent full validation and policy/observation work. The lower
overall phase time comes with frozen-input admission and is not evidence of a
faster executor.

## Action contribution and future credit

Two requested views answer different questions. Exact direct contribution is
`100 * (best_area_before - best_area_after) / initial_area`, summed by disjoint
action kind and averaged over the paired factories. Single and group poses are
separate categories. Their sum is checked against the measured outcome at every
checkpoint for both executors.

| Batch-32 direct contribution | Previous | Region | Difference |
| --- | ---: | ---: | ---: |
| Empty-strip contraction | 46.91 pp | 48.60 pp | +1.69 pp |
| Single-facility pose | 4.32 pp | 5.90 pp | +1.57 pp |
| Group pose | 2.22 pp | 3.49 pp | +1.27 pp |
| All other kinds | 0.00 pp | 0.00 pp | 0.00 pp |

The user's proposed future-credit measure assigns every improvement to every
preceding action, including the action that produced it:
`G[t] = sum(reward[u] for u >= t)`, with gamma one. The analysis recomputes it and
checks equality with every retained `reward_to_go`. Report both its sum by kind
and mean per selected action. The sum counts an outcome repeatedly and does not
sum to the final factory score.

Early actions naturally have more future reward available. A second descriptive
mean subtracts the average `G[t]` at the same action index for the same executor
and checkpoint. This removes the average position effect, not differences in
factory state, policy selection or causal usefulness.

| Region batch-32 action | Count | Mean future gain | Position-centered mean |
| --- | ---: | ---: | ---: |
| Contract empty strips | 133 | 19.58 pp | +4.34 pp |
| Single-facility pose | 201 | 4.19 pp | -0.34 pp |
| Group pose | 120 | 3.34 pp | -1.68 pp |
| Insert strips | 113 | 6.43 pp | +0.73 pp |
| Restore best | 254 | 7.59 pp | -0.93 pp |
| Change port | 13 | 8.12 pp | +1.87 pp |
| Reroute | 98 | 5.93 pp | -1.41 pp |
| Move component | 74 | 4.45 pp | -0.60 pp |
| Release bridge | 18 | 5.20 pp | +0.24 pp |

Insertion has no immediate best-area gain but is associated with later gains,
which is the kind of signal the future-credit view can reveal. Conversely even
rejected actions receive later reward: only one of 74 component actions validates,
yet its raw mean future gain is 4.45 pp. Therefore these plots identify candidates
for investigation; they do not establish that an action caused the later gain or
that a negative centered mean makes an action harmful. Port/release estimates
also have especially small sample counts. Figures are `action-contributions.png`
and `future-action-credit.png`, with standalone SVG counterparts.

## Contract and implementation

The policy selects facility poses. The native executor owns a bounded chain set,
fixed outside contacts, compatible port trials and private repair attempts. The
old one-pass edit repair was removed. Each retry starts from the original witness
and shares the original 5,000 route-expansion allowance. Only the existing native
full-factory acceptance gate can publish a candidate. Current and best factories
remain unchanged after rejection.

Defaults are four attempts, 64 total chains, eight additional chains, four port
trials and eight crossings per route. Each expansion adds at most two encountered
blocking chains. Pipes route first; within each layer the failed chain is first,
then decreasing endpoint Manhattan distance and stable chain ID. Explicit port
assignments remain locked. Outside facilities, terminals and junction positions
remain fixed. The edit's current measured extent remains its local neighborhood.

This is the authorized local improvement experiment on complete validated
factories. It preserves incremental constructive initialization, the area/tiles/
turns objective priority, and the separation from exact research solvers in
`docs/designs/2026-09-02.00-constructive-layout-planner.md`. The policy's observation,
output dimensions and reward are unchanged. Private retry ownership makes the
contract simpler: the policy no longer needs to select every blocking route.

The treatment bundles region retries with changed initial routing order and a
per-route crossing allowance. Its effect cannot be attributed to region expansion
alone. The initial region may succeed without any expansion.

## Selected executor regressions

All cases use the original retained evaluation-208 factory, the same request,
and 5,000 route expansions. Full native validation, deterministic replay,
unchanged outside chains and fixed directional contacts passed.

| Case | Attempts | Expansions | Outcome |
| --- | ---: | ---: | --- |
| Retained distant facility move | 1 | 4,964 | Valid, initial routing order/crossing allowance sufficient |
| Single facility moved to (6,6) | 2 | 1,660 | Valid after automatic compatible output-port trial |
| Crossing component moved two cells | 3 | 3,178 | Valid after adding three surrounding chains |

These are valid exploratory transitions, not area improvements. The retained
distant move changes score `[1672,346,94]` to `[1672,456,108]`. Region recovery alone
does not prove useful optimization. Detailed inputs and reports remain under
ignored `artifacts/factory-edit-pilot/transaction-region-pilot/`.

## Controlled training method

Baseline: the first 32 batches of the retained compact one-hour run, source
`43c5d73`, initialization seed 17. Treatment runtime: `0fd7b70`, incorporating
normal-duration fix `71c731d` and native region implementation `8cf8506`.

Both use fresh identical actor/critic parameters, CUDA FP32, eight workers,
32 attempted factories per batch, 32 actions per episode, unchanged PPO settings,
and evaluation at batches 0/8/16/24/32. The treatment's schedule ends at batch 32.
Readiness and normal-duration probes are separate runs with separate weights.

Equal generation seeds would be insufficient because random setup itself calls
the changed executor. Instead, freeze the baseline's first 1,024 training starts
and 32 evaluation starts. Recover 819 missing starts with the unchanged baseline
runtime and verify exact original hashes; reuse the remaining retained inputs.
The manifest preserves original admission rejection outcomes. Each accepted start
is copied into the new run and admitted by the new native validator. Frozen inputs
and source hashes are included in the immutable protocol.

Comparisons use mean per-factory best-area reduction from the identical initial
factory. Learning deltas subtract each executor's own untrained-policy score.
The action trajectory includes all 32 selected actions, including failures, rather
than retaining only successful edits. A distant-pose statistic means an envelope
origin Manhattan displacement of at least eight cells; it overlaps group poses.

Frozen admission avoids reconstruction/setup cost during treatment collection.
Consequently wall-time throughput is not a controlled executor speed comparison.
Report episode worker time, native executor time and PPO update time separately.
There is only one initialization seed; paired factories are not independent
training replications.

## Reproduction and retained evidence

The server runtime is `/home/ashrimp/Srv/aic-ppo-region-20260910`. It uses the existing
locked Python 3.12 environment and a fresh release native build. Source binaries
for the unchanged constructor CLI and witness probe were copied from the frozen
baseline runtime. The baseline's native library and source files were untouched.

```sh
python tools/factory-edit-pilot/freeze_ppo_starts.py \
  --baseline "$BASELINE_RUN" --native-root "$BASELINE_ROOT" \
  --output "$FROZEN_STARTS" --batches 32 --workers 8
# Place the resulting manifest at the config's explicit input_manifest path.
python tools/factory-edit-pilot/ppo_harness.py --env-file "$ENV_FILE" prepare \
  --config tools/factory-edit-pilot/ppo-region-32-batch.json --output "$REGION_RUN"
python tools/factory-edit-pilot/ppo_harness.py --env-file "$ENV_FILE" run \
  --run "$REGION_RUN"
python tools/factory-edit-pilot/compare_region_pilot.py \
  --baseline "$BASELINE_RUN" --region "$REGION_RUN" --batches 32 \
  --output "$EVIDENCE/comparison.json"
python tools/factory-edit-pilot/plot_region_pilot.py \
  --summary "$EVIDENCE/comparison.json" --output "$EVIDENCE/view"
python tools/factory-edit-pilot/audit_region_pilot.py \
  --run "$REGION_RUN" --output "$EVIDENCE/audit" --env-file "$ENV_FILE"
```

Preserve the frozen initial-input manifest and files, source runtime, durable
records, non-replayable trained checkpoints, all evaluation trajectories and
the explicitly retained training batches. Other training intermediates use the
existing automatic retention cleanup. Generated reports and figures stay ignored;
the companion compact provenance manifest records their hashes and locations.

## Verification

- `cargo fmt --all`, `cargo check --workspace` and `cargo test --workspace` passed:
  548 tests, zero failures, five ignored.
- The release external regression command passed all three selected transactions:
  `AIC_REGION_CASE_DIR=artifacts/factory-edit-pilot/transaction-region-pilot/regression-inputs cargo test --release -p aic-pilot-native retained_region_transactions_validate -- --ignored`.
- All 50 `test_ppo*.py` tests passed against the fresh native debug extension.
- CUDA readiness completed two batches; native audit passed 24 results, saved
  behavior probabilities/values, and exact continuation from nonempty Adam state.
- A separate online `--duration-seconds 20` run ended `stopped/duration-reached`,
  returned exit code zero, retained its initial committed checkpoint, and had
  coordinator/observer exits `[0,0]`. W&B confirmed `finished` with
  `training_complete=false` and `termination_reason=duration-reached`.
- Re-exporting the previous one-hour run with the fixed observer corrected its
  W&B state to `finished`; its original training records and checkpoint were
  preserved. Historical stop reason remains `stopped` rather than retroactively
  inventing a duration field in its frozen training state.
- Final audit verified frozen source hashes, all 33 checkpoint identities and
  restoration of batch 32, all 160 native evaluation results and their reward
  sequences, and all 1,435 W&B scalars across 42 records. The run is `finished`,
  with `training_complete=true`, `termination_reason=schedule-complete` and child
  exits `[0,0]`: `b43bee8ae8ba92a289a7b106`.
- Comparison/plot CLIs passed a same-run readiness check with zero paired
  differences, then the complete paired run. Generated plots were visually
  inspected. Direct contributions sum exactly to each outcome; future credit
  matches the recorded reward-to-go.
