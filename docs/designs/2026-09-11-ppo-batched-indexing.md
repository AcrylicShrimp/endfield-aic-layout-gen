# PPO batched spatial indexing

Status: complete. See the [measured results](../reports/2026-09-11-ppo-batched-indexing.md).

The user requested grouping the remaining per-factory GPU indexing operations.
The existing learner already batches 64 recorded selections, combines independent
graphs and groups equal-shaped rasters for CNN execution. This change extends
batching inside those encoders; it does not introduce batching for the first time.
Long training remains stopped. Use bounded isolated CUDA diagnostics.

The user subsequently requested comparison with padding to each minibatch's
maximum height/width and an explicit valid-region mask. Include this candidate:
mask normalization statistics, intermediate activations and spatial means, not
just the original input. Grid normalization must still use each actual factory's
extent. Measure padded/valid pixel counts and memory; no global canvas bound is
introduced. Different floating reduction orders may produce roundoff: compare
against the existing forward/gradient tolerance checks as well as exact hashes,
and explicitly report any loss of bit-exact continuation.

## Contract

- Inputs: the same immutable observations, current/best relationships, actor
  contracts, candidate ordering, graph membership and preceding model/Adam/RNG.
- Prepare integer spatial addresses on CPU. Each address identifies both its
  source world and its pixel within a CNN batch. Padding must be excluded from
  spatial normalization; never mix information between factories.
- Gather node, action and member features across worlds within a bucket. Restore
  the original concatenation order. Missing actor worlds keep zero features;
  empty member sets and repeated coordinates remain valid.
- Outputs: unchanged actor probabilities, critic values and gradient meaning.
  The initial bit-exact retained CUDA PPO continuation target was not met.
  The user-authorized padded execution preserves mathematical semantics, with
  explicit floating-point differences measured below. Keep strict failures as
  evidence; do not claim old checkpoint hashes or trajectories remain identical.
- Preserve CNN/graph depth and width, parameter initialization, FP32 policy,
  shuffle/minibatch order, four epochs, KL checks, reward and curriculum semantics.
- Keep former execution only in an explicitly loaded frozen benchmark runtime.
Remove obsolete internal per-world coordinate DTOs when the cutover succeeds.

## Evidence and acceptance

Baseline: commit `d22474b`, frozen server runtime
`/home/ashrimp/Srv/aic-ppo-data-supply-20260911-r2`. Use its retained batch 3
(61 factories / 1,952 selections) with nonempty Adam and mixed inheritance depths.
Isolate baseline batch/world/supply/update modules so current imports cannot
accidentally change the comparison implementation.

Measure warmed resident minibatch execution, preparation/transfer, complete PPO
updates, peak CUDA allocation and kernel launches separately. Compare numerical
outputs and all parameter gradients before measuring full optimizer continuation.
Record repeated measurements, source/input hashes and rejected variants under
ignored artifacts. A full native readiness run must verify integration if the
new learner path passes. Keep documentation/provenance compact and commit after
PPO regressions, workspace checks/tests and artifact-policy checks pass.

## Numerical cutover

Learner execution schema is `ppo-batched-input-contracts-v3`. The model's 28
input channels and checkpoint parameter structure are unchanged. An explicit
`B x 1 x H x W` mask accompanies padded rasters; it is not a learned input channel.
Each factory retains its own coordinate normalization and graph membership.
The old per-world GPU action/member coordinate DTO is removed.

The frozen implementation remains the reference for forward/gradient tolerance
checks. Full Adam continuation also reports exact hashes and strict numerical
comparisons, but those comparisons fail for the padded candidate. The largest
parameter discrepancy is the final scalar score bias: a common logit offset
cancels mathematically in softmax, while tiny numerical gradients can move its
Adam state. Other parameters and final behavior probabilities also differ; do
not dismiss all differences as that redundant bias. PPO clipping thresholds can
amplify roundoff and change discrete diagnostic fractions.

The benchmark's explicit `--allow-roundoff` mode measures these failed strict
comparisons without hiding them. It still requires the historical baseline to
reconstruct exactly, repeated executions of each implementation to reconstruct
exactly, and the recorded selection count, optimizer-step count, KL stop decision
and shuffle state to agree. Evaluate all retained selections after optimization.
Native integration must independently validate factories, collector/learner
behavior probabilities and exact continuation within the new runtime. This is
bounded implementation evidence, not a claim of unchanged long-run learning.
Existing frozen runs must continue through their original runtime.
