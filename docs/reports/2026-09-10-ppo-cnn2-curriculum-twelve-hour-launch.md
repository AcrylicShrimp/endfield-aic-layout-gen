# CNN-2 best-state curriculum: fresh twelve-hour launch

Follow-up: the user stopped this run normally at 32 batches / 64,608 actions to
prioritize [data-supply optimization](../designs/2026-09-10-ppo-data-supply.md).
Its final checkpoint identity and completed W&B export were verified. The
twelve-hour allowance was not completed. The launch snapshot below is historical.

The 64-factory experiment started from the original untrained seed-17 CNN-2
initialization at **2026-09-10 23:02:22 KST**. Its new 43,200-second allowance ends
nominally at **2026-09-11 11:02:22 KST**, followed by bounded finalization. This
report verifies launch and the first four batches; twelve-hour training remains
in progress.

The preceding routing/reward run stopped normally with `user-stop` after 701
batches and 704,128 admitted training actions. Its final checkpoint hash was
verified, and its checkpoint, committed records and partial work remain in its
original runtime. W&B export finalized successfully.

## Experiment contract

Each batch attempts 64 factories with 32 actions each. Mature scheduling assigns
80% fresh starts, 10% best states after one completed pass and 10% after two
completed passes. Integer quotas balance over five batches. Early batches use
explicit fresh substitutions while parent pools form. Pass depth counts completed
episodes; a parent need not have achieved a positive area gain.

Only factory states and their original obligations are inherited. Collectors
sample new trajectories under the current actor and critic, with fresh episode
history. Consecutive parent identities and immutable best-state bundles survive
ordinary output cleanup. The [contract](../designs/2026-09-10-ppo-best-state-start-curriculum.md)
defines exact selection, admission, shortage and reward-denominator semantics.

The run retains the two-layer CNN, eight CPU collectors, CUDA FP32 learner,
50,000 routing-work budget, 0.001 failure penalty and existing PPO settings.
Evaluation uses the same fixed 32 factories every four batches. The first 16
training batches span the previous run's 1,024 scheduled frozen-input slots;
inherited slots replace some fresh inputs. Batch size and start distribution
change together, so this experiment cannot isolate their individual effects.

## Verified launch

All initial actor/critic tensors and CPU/CUDA/shuffle RNG states exactly match
the original untrained checkpoint. Both Adam states are empty. No trained model,
optimizer or random stream was imported from the stopped run.

The [readiness report](2026-09-10-ppo-best-state-curriculum-readiness.md) records
67 passing PPO tests, workspace checks/tests, 320 native result audits and exact
reconstruction of the third PPO update from nonempty Adam state, including a
two-pass episode's probabilities and values.

The actual twelve-hour run then committed:

| Batch | Fresh attempts | One-pass attempts | Two-pass attempts | Admitted actions |
| --- | ---: | ---: | ---: | ---: |
| 1 | 64 | 0 | 0 | 2,048 |
| 2 | 58 | 6 | 0 | 2,016 |
| 3 | 51 | 7 | 6 | 1,952 |
| 4 | 51 | 6 | 7 | 1,984 |

All 32 inherited factories were admitted. An independent record audit confirmed
their consecutive ancestry, exact parent best geometry and metrics, current
behavior-policy identities, deterministic start plans and evaluation separation.
Six fresh inputs were rejected during generation. No post-bootstrap donor shortage
occurred in this snapshot.

Remote W&B configuration confirms 64 attempts and the 80/10/10 curriculum.
All exported per-depth, first-eight/later-action, physical-repeat and action-kind
area-contribution metrics for four training phases and evaluations 0/4 match
the local committed records. Fixed-cohort mean area reduction was 47.1164% at
initialization and 50.7698% after four batches. This early measurement establishes
working training and tracking; comparative policy quality remains unmeasured.

## Runtime and reproduction

Implementation commit: `a9af915`. Frozen runtime:
`/home/ashrimp/Srv/aic-ppo-cnn2-curriculum-64-12h-20260910`.

Configuration: `tools/factory-edit-pilot/ppo-cnn2-curriculum-twelve-hour.json`.
Run: `artifacts/factory-edit-pilot/cnn2-curriculum-twelve-hour`.
User service: `aic-ppo-cnn2-curriculum-64-12h-20260910.service`, `Restart=no`.
User lingering is enabled; the service continues after SSH or conversation closure.

[W&B live run](https://wandb.ai/acrylicshrimp-personal/endfield-aic-layout-gen/runs/c8f5c158d76d3261d2804785).
The adjacent provenance manifest binds protocol, initialization, native binary,
predecessor checkpoint and live verification evidence. Detailed evidence is under
the runtime's ignored `artifacts/factory-edit-pilot/curriculum-launch-evidence/`.

From that runtime, inspect progress or repeat the committed-lineage check:

```sh
PY=tools/factory-edit-pilot/.venv/bin/python
RUN=artifacts/factory-edit-pilot/cnn2-curriculum-twelve-hour
$PY tools/factory-edit-pilot/ppo_harness.py status --run "$RUN"
$PY tools/factory-edit-pilot/audit_ppo_start_curriculum.py --run "$RUN" \
  --output artifacts/factory-edit-pilot/curriculum-lineage-recheck.json
```

To reproduce a separate fresh run, use the public `prepare` and `run` commands
with the tracked configuration, a new output directory and the existing environment
file. Supply `--duration-seconds 43200` to `run`; do not reuse this active output.
The schedule has 8,192 batches of headroom; duration and existing resource limits
determine actual completion. Normal finalization preserves the last committed
checkpoint and partial non-replayable work.
