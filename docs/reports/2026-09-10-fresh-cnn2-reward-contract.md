# Fresh CNN-2 reward and comparison contract

The new twelve-hour experiment starts from random initialization, seed 17. It
imports no actor, critic, optimizer or random-stream state from the stopped run.
Its two-layer CNN, graph encoders, observation contract, PPO hyperparameters,
32-action horizon and 32 held-out factories match the original CNN-2 experiment.
The initial model identities must match the original pre-training checkpoint;
evaluation outcomes may differ because the routing executor has changed.

For a controlled early comparison, the first 32 training batches reuse the exact
frozen factory inputs and generation rejections from the original CNN-2 run.
Training and action seeds begin at the original bases 211000000 / 221000000.
After batch 32 the schedule generates new inputs, continuing those seed sequences.
The new explicit `prefix-and-evaluation` input scope freezes the declared prefix
and all evaluation seeds; a missing prefix entry is an error, never an implicit
fallback to generation. Beyond the prefix, initial geometry can differ because
random setup now uses the improved router. Consequently the full twelve-hour
comparison is not a fully paired per-trajectory causal experiment.

The previous 12-hour continuation started from an already-trained 32-batch model.
Compare the new run against the original CNN-2 origin and the old continuation's
cumulative action/batch curve, not against that continuation's local batch-zero
quality. Changes to routing and reward are evaluated together; this experiment
does not isolate each component's contribution to eventual learning quality.

## Reward DTO and invariants

The PPO objective is explicitly `ppo-best-area-with-failure-cost-v1`:

```text
area_reward_t = (best_area_before_t - best_area_after_t) / initial_area
failure_cost_t = 0.001 if status is executor_rejected, rejected or late; else 0
reward_t = area_reward_t - failure_cost_t
```

The coefficient is configured as `reward.failure_penalty` in the harness JSON.
Allowed values are finite numbers in [0, 0.01]; the new run uses 0.001. Earlier
configurations without a reward field normalize to zero cost when preparing a
new run. This does not modify already-frozen protocols or experiences.

An accepted equal/exploration action has no failure cost, including a valid move
whose best area does not improve. A rejected action preserves best area and has
zero area reward. Late/incomplete episodes retain partial evidence and remain
excluded from complete PPO batches. Search cutoffs are still penalized weakly,
which is why reducing false rejections is part of the same change.

Each trace/experience step stores `status`, `area_reward`, `failure_cost` and
`reward`. Episode rows persist `reward_schema`, `reward_config`, `reward_sum`,
`area_reward_sum`, `failure_cost_sum` and `failed_actions`. Area reward still
telescopes to the fractional best-area improvement; total reward instead subtracts
accumulated failure cost. Ten failures cost 0.01 of normalized return. An all-failed
32-action episode returns -0.032, while a 50% area improvement contributes +0.5.

The current PPO collector requires the explicit reward contract and validates
its step/episode conservation before GAE or optimizer updates. Negative returns
are supported. Archived pure-area studies retain their separate schema and
reference tooling; old payloads are not silently imported into the new collector.
The native geometry/acceptance layer continues reporting its original area
quantity, while the PPO rollout applies the declared learning objective.

## Persistent diagnostics

Every completed episode retains compact routing failure-code counts, unique
budget-failed action count, first-attempt budget failures, executor time and
preflight/total expansions. Phase summaries and W&B publish these alongside
failure fraction and separate reward components. The detailed action count is
explicit, so a summary with missing historical fields is not presented as full
cause coverage. Nested local/region budget codes count once per failed action.
Regenerable detailed traces can be cleaned without losing these aggregate rates.

## Verification and execution

Reward tests cover weak negative returns, GAE on failure-only episodes, accepted
exploration neutrality, independent area conservation and rejection of tampered
components/configurations. Frozen-input tests cover exact prefix routing, missing
entry failure and the switch to fresh generation outside the declared prefix.
All 60 PPO tests and workspace checks/tests pass. The launch additionally requires
a bounded CUDA readiness run, native input/best validation, recorded behavior
probability/value checks and exact reconstruction of the second PPO update.

Configuration: `tools/factory-edit-pilot/ppo-cnn2-routing-twelve-hour.json` with
`random-factory-routing-50k.json`. The route budget is 50k, with the native 25k
per-attempt ceiling and bounded reverse reachability described in the
[routing report](2026-09-10-routing-cost-and-bounded-reachability.md).
The actual frozen runtime, source identities, new W&B run and start/end times
belong to the subsequent launch report.
