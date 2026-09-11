# Lower PPO learning-rate screen

Status: registered before execution.

Readiness passed before the main run. Both two-batch cells completed in 46.54
seconds, with all 96 native result records valid and exact second-batch update
replay from nonempty Adam state. Fresh and restored optimizer groups have the
declared rate for both learners. Both forced tails used one actor epoch and four
critic epochs with the actor frozen. All 93 Linux/CUDA regression tests passed.

Readiness evidence resides under
`/home/ashrimp/Srv/aic-ppo-learning-rate-readiness-20260911/artifacts/factory-edit-pilot/`:

| Report | SHA-256 |
| --- | --- |
| `audit-1e-4/report.json` | `7eb17921ebb2a320009764564b97a5cc2e1b1ef56bbea0872f373d4082062491` |
| `audit-3e-5/report.json` | `c75410b487fea3e10e40a033084b67af81cd52eb967ca5ee0a1267370248a02e` |

## Contract

Compare fixed Adam learning rates 0.0001 and 0.00003 against the retained
0.0003 shared width-40/history/critic-tail control from the completed capacity
screen. Change both actor and critic learning rates together. Keep two CNN
layers, both shared encoders, critic history, critic tails, actor history 16,
hidden actor best-world input, reward and every other PPO/workload setting fixed.
No learning-rate schedule, higher rate, inherited start or resumed training.

The selected architecture is the best wider-shared condition in the preceding
two-seed screen. This is an exploratory conditional comparison, not evidence
that it is the globally best architecture or that another architecture would
respond identically to learning-rate changes.

Train each new rate from scratch with seeds 17 and 29, 32 batches, 64 factory
attempts per batch and 32 actions per admitted factory. Evaluate at batches
0/8/16/24/32 on the same 32 attempted factories per seed. Reuse the completed
0.0003 runs as controls; verify their report and all referenced file identities.
Keep runtime packages, native binaries and original game/request data fixed.

Inputs are the versioned harness configuration and the explicit
`--learning-rates` matrix mode. Output is a `ppo-learning-rate-matrix-v1`
two-cell matrix with rates, immutable protocol/configuration identities, job
states, exit codes and structured diagnostics. Register the two rates in code;
reject mismatched model factors or an incompatible base configuration. Existing
factorial and shared-capacity matrix contracts keep their original semantics.

Run a two-batch native/CUDA readiness at both rates before the main matrix,
including exact resumed updates and verification of the actual Adam parameter
group learning rates. Freeze the main source in a separate server tree and
execute the four runs sequentially under the existing resource limits. A failed
cell stops the matrix for investigation. Do not silently shorten or restart it.

## Required evidence

- All new results and checkpoint identities audited, including native final
  factory validation, reward/prediction accounting and critic-tail actor freezes.
- Identical input geometry and initial actor/critic parameters across all three
  rates within each seed. Require identical initial evaluation results and
  first-batch collection before different optimizer steps begin.
- Full configuration equality except learning rate, matching native/data inputs,
  and runtime source changes restricted to declared tracking display changes.
- Final best-area reduction, learning curves, paired seed differences, late
  critic prediction error and selected-action failure rates.
- KL stopping counts, first-epoch KL, actual actor/critic epochs and optimizer
  steps, clipping diagnostics and measured training/update time. Equal experience
  does not imply equal update work when KL stopping changes.
- W&B completion and scalar/configuration checks for all four new runs.
- English report, compact provenance, reproduction commands and appropriate
  source checks committed. Keep generated outputs and checkpoints outside Git.

Two seeds and reused evaluation factories do not establish significance or
provide an untouched confirmation set. The seed controls initialization and
factory/action draws; compare rates within a seed. Both actor and critic rates
change, so the experiment does not isolate which learner causes a difference.
Value error is against each policy's own realized remaining rewards. Timing is
descriptive because retained controls were not run concurrently.
