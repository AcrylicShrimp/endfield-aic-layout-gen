# Frozen-policy structural band masking screen

Status: completed; registered before the outcome comparison. Results are in
`../reports/2026-09-14-frozen-policy-band-mask-screen.md`. The user authorizes starting
an original-versus-masked experiment after discussion of a frozen-model screen.
This stage changes action selection at evaluation only; it does not train or
resume the main run.

## Question and fixed comparison

Does excluding structurally impossible bands improve retained validated factory
area within the existing evaluation budget, and at what integrated runtime cost?
The preceding diagnosis found these rejections in 40.80–55.74% of final actions,
but did not establish that they caused the late score decrease.

- Models: baseline-8 update 128, initialization seeds 71 and 113, from the
  completed cold study. Same frozen weights in both conditions for each seed.
- Inputs: the existing confirmation panel, all 32 distinct geometries, 16 per
  size stratum. These geometries were reserved from both source training runs.
  This is explicitly exploratory reuse, not a fresh confirmation of a new
  training algorithm or an unseen-population generalization claim.
- Actions: four fixed draws per geometry, panel action seed plus 0/1/2/3. Draws
  0/1 also support comparison to retained original reports. Both variants use
  identical action seeds; this does not require identical sampled trajectories.
- Conditions: original and structural-band-mask. 512 measured episodes total
  (2 models × 2 conditions × 32 geometries × 4 action replicates).
- Budget: 64 charged decisions, 10,000 ms native session allowance, original
  route limits and all other native/model/input settings. The experiment has a
  1,200-second execution ceiling, 32 GiB process-tree RSS limit, 16 GiB free-disk
  reserve, and a 2 GiB retained-snapshot ceiling checked after each block.
- Execution: 16 persistent workers on distinct physical CPU cores, original
  worker-local CPU inference and evaluator, shared-memory transport and cached
  case admission. No generator calls or optimizer construction/update.
- Warmup: one representative case per worker in each condition; 32 episodes
  excluded from outcomes. Retain warmup reports and remove their raw snapshots.
- Measured order: seed71 original, seed71 masked, seed113 masked, seed113 original.
  This balances the condition order across the two frozen models.

## Mask contract

The mask sets a proposed band's logit to a finite negative sentinel that yields
zero sampling probability when the native structural checks would reject it:
cutting any part of a rigid facility or original logistics component, or deleting
an original non-facility anchor. Insertion may occur at a body's boundary but
cannot split its interior. Component footprints and rotations use the external
catalog and current observation. Non-facility anchors include the native
contract's component/routing nodes as well as external terminals.

Preserve the complete candidate list, indices, graph features, current-state
preparation, conditional port decoder, and original per-kind count adjustment.
Thus this intervention conditions the original root distribution on surviving
actions; it does not separately reweight action kinds by changing their counts.
This is a more isolated intervention than filtering and rebuilding the domain
mentioned as a possible approach in the preceding diagnosis.

Every placement remains available, including overlap and temporarily worse or
routing-pending preparation. Do not consult a routing solver, score, retained
best, future outcome, or heuristic desirability to construct the mask. Existing
native validation remains authoritative. A masked rollout emitting any of the
three targeted rejection codes fails with `band-mask.unexpected-rejection`.
Other rejection reasons remain possible and are reported.

Geometry-dependent blocked coordinate sets are cached only for the current
episode's blueprint/revision and configured widths. The per-action mask is
aligned with the actual current candidate list. The finite logit sentinel avoids
an undefined zero-times-infinity entropy term in the existing sampling kernel.
All candidates and model interfaces remain compatible; no replay is loaded into
training or relabeled. Supporting masked PPO training would be a separate change.

## Evidence, continuity, and decision

| Existing benefit | Reuse | Check |
| --- | --- | --- |
| Frozen model and evaluation/input separation | Load model tensors only from bound completed checkpoints; verify reserved panel identities | Checkpoint/launch/model hashes; no training operation accepted by research worker |
| Persistent parallel workers, compact transport, cached inputs | Existing `BlueprintPool`, worker protocol and case cache | Actual worker PIDs/core affinity; block wall and worker CPU measurements |
| Original joint policy distribution | Root-logit view over original `ActorCritic`; original counts and conditional ports | Probability-conditioning/finite-entropy test; unmasked sampling equivalence |
| Native validity and interruption handling | Existing native evaluator, worker failure preservation and resource guard | All retained outputs validated; guards and targeted rejection counts |
| Efficient evidence storage | Existing compressed snapshot writer and one-pass checkpoint inflation | Actual saved bytes; end-to-end episode/block cost includes writing |
| Main and completed study stay recoverable | New research-only files/output root; frozen source imported read-only | Source bindings and checkpoint hashes; no diagnostics added to normal execution |

Before the measured comparison, check structural boundary cases, rotated
components, facility-port exemption, cache invalidation, unchanged placement
availability, and conditional probabilities. A separate bounded native fixture
audit compares every proposed band against the native rejection result from the
same original state. This audit does not run inside evaluation episodes.

Primary outcome is equal-size-stratum mean area reduction. Average action draws
within geometry, then average the two model seeds. Reuse the study's paired,
size-stratified geometry bootstrap (10,000 draws), attaching all action/model
replicates. Report each model, each size stratum, geometry wins/ties/losses,
rejection/pending/overlap counts, valid-output coverage, mask preparation cost,
worker CPU, block wall time, and retained bytes. Baseline repeat differences are
reported; elapsed time is an existing model feature, so bit-exact reproduction
across different executions is not assumed.

A favorable screen supports a subsequent training experiment; it does not adopt
masking into production or establish that masked PPO will learn faster. A null
or negative result is retained without further tuning on this panel in this run.
