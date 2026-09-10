# Fresh CNN-2 routing/reward experiment: twelve-hour launch

The new run is **running from random initialization**, not continuing a trained
checkpoint. It started at **2026-09-10 19:44:55 KST** and has a fresh 43,200-second
allowance, nominally ending **2026-09-11 07:44:55 KST**, plus bounded finalization.
This report verifies launch and early progress; it does not claim twelve hours
of training have completed.

The former twelve-hour run was stopped at the user's request after 526 additional
batches / 528,256 admitted training actions. Its last checkpoint, complete records
and partial batch 527 remain preserved. Its supervisor and W&B export finalized
normally with `user-stop`.

## Configuration and comparison

- Random initialization seed 17; unchanged two-layer CNN, two graph layers,
  width 32, `current-history-16` actor input, critic architecture and PPO settings.
- No continuation field and no imported actor, critic, optimizer or random stream.
- Routing work 50,000; 25,000 per-attempt ceiling; bounded directed reachability
  charged to the same work budget. Complete candidates still require validation.
- Failed actions cost 0.001 normalized reward; area progress and failure costs
  remain separately recorded. Accepted exploration has no failure cost.
- First 32 training batches use the original frozen initial factories, including
  the original generation rejections. Training then generates fresh factories.
  All evaluations use the same fixed cohort of 32 factories.
- Eight collection workers, CUDA FP32 learner, 32 attempts per training batch and
  32 actions per episode. The large schedule is headroom for the twelve-hour
  duration; it is not a promise to complete all scheduled batches.

The [reward/comparison contract](2026-09-10-fresh-cnn2-reward-contract.md) explains
why comparison starts at the original random CNN-2 model, and why post-prefix
training is not a fully paired causal experiment. The [routing report](2026-09-10-routing-cost-and-bounded-reachability.md)
records measured cost, the selected-failure matrix, proof obligations and remaining
local failures.

## Launch validation

A separate readiness run completed two eight-episode training batches and two
32-episode evaluations: 512 training actions and 80 native result audits. The
audit independently validated initial/best factories, checked collector behavior
probabilities and critic values, and exactly reconstructed the second PPO update
from its nonempty optimizer checkpoint with the new negative rewards.

Every actor/critic tensor and initial random stream in the readiness checkpoint
matched the original pre-training CNN-2 checkpoint. Optimizer states were empty.
The main run's initial model hashes match those same values and its optimizers
are also empty. The main initial evaluation reproduces readiness outcomes and
reward/status accounting. The first actual training batch matches the frozen
input identities; the first post-prefix seed was separately generated and admitted
without using a frozen entry.

At verification the main run had four committed training batches / 4,064 admitted
actions. W&B was online and running, with reward components, failure fraction and
routing budget failures confirmed in remote history. Later live progress may
exceed this frozen launch snapshot.

With the **same untrained policy and initial evaluation factories**, the old and
new executors give:

| Initial evaluation | Original 5k router | Improved 50k router |
| --- | ---: | ---: |
| Actions | 1,024 | 1,024 |
| Executor rejections | 149 (14.55%) | 142 (13.87%) |
| Budget failures | 53 (5.18%) | 8 (0.78%) |
| Mean area reduction | 47.33% | 47.12% |

Budget failures fall substantially, while other rejection causes remain. The
changed router alters later states/actions, so this is a matched starting-policy
comparison, not a forced replay of identical complete trajectories. It does not
establish improved final learning quality; the fresh run will measure that.

## Runtime and monitoring

Frozen source commit: `d434768` (routing implementation `9e3ce33`). Runtime:
`/home/ashrimp/Srv/aic-ppo-cnn2-routing-12h-20260910`.

Run: `artifacts/factory-edit-pilot/cnn2-routing-twelve-hour`.
User service: `aic-ppo-cnn2-routing-12h-20260910.service`, `Restart=no`.
User lingering is enabled so SSH or conversation closure does not terminate it.
Session: `ac1e006e3fd446e8b5ba5e852a92086d`; initial supervisor PID 652194.

[W&B live run](https://wandb.ai/acrylicshrimp-personal/endfield-aic-layout-gen/runs/5ed736ee4ac1e4ae121f87b3).
The adjacent compact manifest binds protocol, native binary, initialization,
readiness audit, initial evaluation comparison and stopped predecessor identities.
Detailed evidence remains under the runtime's ignored
`artifacts/factory-edit-pilot/launch-evidence/`.

From the frozen runtime, inspect or explicitly stop the run with:

```sh
PY=tools/factory-edit-pilot/.venv/bin/python
RUN=artifacts/factory-edit-pilot/cnn2-routing-twelve-hour
$PY tools/factory-edit-pilot/ppo_harness.py status --run "$RUN"
$PY tools/factory-edit-pilot/ppo_harness.py stop --run "$RUN"
```

The normal duration stop preserves the last committed checkpoint and partial
non-replayable evidence. Resume, if later explicitly requested, uses this new
run's own protocol/checkpoint; it must not import the old trained model. No
periodic notification automation was requested or installed.

Source verification: all 60 PPO tests, workspace checks/tests, the independent
path-quality oracle and artifact-policy checks passed. Generated intermediate
variant candidates were removed after retaining their measured reports; original
and final selected witnesses, captured path inputs and checkpoints were preserved.
