# Hierarchical Gumbel candidate-width comparison

Status: all 21 predeclared search cells completed on the three frozen
16-case train-source panels. This is the candidate-coverage stage of the
[causal study](../designs/2026-09-25-hierarchical-target-causal-study.md).
No model was trained and no production search setting was changed.

## Question and method

Does the 16-action root cap exclude actions that a wider Gumbel search would
choose, and does visiting more actions give the policy learner a more
informative target? The panels, generation-87 lower and outer weight bytes,
complete first-root action sets, root seed, eight guaranteed anchor actions,
root prior mix, search horizon, and move limit were fixed. Candidate widths
16, 32, and 64 were compared at both 128 and 512 simulations per move;
128 candidates were also tested at 512 simulations. Every paired first-root
state and complete action-witness set matched. The considered sets were nested
at a fixed simulation budget. Search used the same release binary as the
[previous target audit](2026-09-25-hierarchical-target-quality-audit-result.md).

## Result

| Candidates / simulations | Completed / 48 | Gains / losses vs 16/128 | Selected outside original 16 | Mean first-root mass on unvisited actions | Integrated wall time |
| --- | ---: | ---: | ---: | ---: | ---: |
| 16 / 128 | 19 | 0 / 0 | 0 | 99.47% | 169 s |
| 32 / 128 | 20 | 2 / 1 | 0 | 99.03% | 192 s |
| 64 / 128 | 19 | 4 / 4 | 0 | 97.35% | 246 s |
| 16 / 512 | 20 | 4 / 3 | 0 | 99.50% | 451 s |
| 32 / 512 | 21 | 4 / 2 | 0 | 98.86% | 493 s |
| 64 / 512 | 21 | 4 / 2 | 0 | 97.93% | 554 s |
| 128 / 512 | 22 | 4 / 1 | 0 | 93.63% | 656 s |

Panel completion counts in arm order are S1 `6,7,7,7,8,8,7`, S2
`7,7,6,7,7,7,8`, and S3 `6,6,6,6,6,6,7`. Across all seven arms and 48 cases,
the final first action always belonged to the original 16; none was one of
the eight guaranteed anchor actions. Thus the modest completion changes do
not show that the wider candidate pool discovered a better *chosen* first
action. Width changes the allocation and later-state search too. For
128/512, the first action was unchanged in 38/48 cases. One of its four
gains occurred with the same first action, so later decisions alone can
change completion. The other three gains and one loss had a different first
action, still within the original 16.

The first-root learned, anchor, and mixed priors were nearly uniform: their
mean effective action counts were 2,733.4, 2,733.5, and 2,733.7 over an
average of about 2,734 available macros. At 16/128, the visited actions held
0.595% of the mixed prior mass and 0.528% of the resulting target mass.
About 12.0 of the 16 visited actions per root had a Q-value below the value
imputed for unvisited actions. This follows the current code's
visit-weighted completed-Q construction; it is an explanation of the target
spread, not proof that any unvisited action is good or bad. The inferred
imputation was checked against the recorded target/prior log ratios and the
search code. The initial root value is a shallow construction-progress value
while visited Q reflects deeper simulations, so replacing imputation with
the initial root value would introduce a depth mismatch.

Widening to 128 candidates at 512 simulations reduced mean first-root target
effective actions from 2,717 to 2,308, but the target still put 93.6% of
mass on unvisited actions. The 16-candidate target became slightly *flatter*
when simulations increased from 128 to 512. Target concentration therefore
does not increase monotonically with search budget under this construction.
This is a diagnostic signal; it is not a correctness judgment. A wider set
also gives each early candidate fewer visits, and these paired completion
differences are small.

## Evidence, limits, and next stage

The [protocol](2026-09-25-hierarchical-candidate-width-protocol.json),
[per-cell progress and raw hashes](2026-09-25-hierarchical-candidate-width-progress.json),
and [compact case evidence](2026-09-25-hierarchical-candidate-width-summary.json)
retain the panel, configuration, model, executable, paired outcome, selected
action, and aggregate target identities. Exact forced-first-action witnesses
for the next stage are retained by stream in the registered benchmark assets
[S1](../benchmarks/hierarchical-target-causal/continuation-actions-s1.json),
[S2](../benchmarks/hierarchical-target-causal/continuation-actions-s2.json), and
[S3](../benchmarks/hierarchical-target-causal/continuation-actions-s3.json).

Search used source commit `dc12943` and release binary SHA-256
`e594b5425074a03731524d8c91991126232d840ef44a86e8b0630e1f16800fdf`.
Descriptive analysis was expanded after the launch in commit `99ceaae` to
record prior flatness, imputation, and anchor selection; it did not change
search inputs or outcomes. The 21 integrated runs took 2,761 seconds and
produced 1,224,737,327 raw report bytes before verification and cleanup.
This was a bounded diagnostic, so W&B was not required. After verifying the
compact result and next-stage action witnesses, the
[cleanup manifest](2026-09-25-hierarchical-candidate-width-cleanup.json)
records 1,050,342,217 removed file bytes and available space rising from
175,398,916,096 to 176,449,462,272 bytes. Three 16/128 raw reports remain
temporarily because the target-construction stage requires their full root
distributions; the other 18 reports and regenerable inputs were removed.

From a fresh checkout under `~/Srv` with the documented LibTorch environment:

```bash
cargo build --release -p aic-cli --features learning
bash tools/factory-edit-pilot/prepare_hierarchical_target_quality.sh \
  target/release/aic-cli artifacts/target-causal-study
python3 tools/factory-edit-pilot/hierarchical_candidate_width.py prepare \
  --output artifacts/target-causal-study --experiment-commit dc12943
python3 tools/factory-edit-pilot/hierarchical_candidate_width.py run \
  --output artifacts/target-causal-study
python3 tools/factory-edit-pilot/hierarchical_candidate_width.py summarize \
  --output artifacts/target-causal-study
```

**Decision:** Keep the 16/128 search setting during the next causal probe.
Force selected and independently sampled first actions, then compare validated
continuations. That will determine whether the wider search failed to select
better outside actions, whether its Q ranking is misleading, or whether the
available first placements have similar downstream outcomes.
