# Cold masked PPO comparison

Status: complete. All four cold arms reached update 128 and all 1,024 final
cross-evaluation episodes completed. Results and provenance are recorded in
`../reports/2026-09-14-cold-masked-ppo-comparison.md` and its companion manifest.
The user authorizes training
original and masked policies from random initialization after the favorable
frozen-policy masking screen. No old learned model, optimizer, successful replay
or training history may initialize a scientific arm.

## Experimental question and fixed allocation

Compare the complete original and masked learning pipelines, then distinguish
learning with a mask from applying a mask only at execution. The registered
configuration is `tools/factory-edit-pilot/operations/declarative-mask-training.json`.

- Four scientific arms: original and masked for initialization seeds 71 and 113.
  The two conditions within a seed have exactly equal initial model tensors,
  empty Adam states, empty replay, and equal initial action/shuffle RNG states.
- Each arm trains for 128 updates of 32 logical streams × 32 actions: 131,072
  decisions per arm, 524,288 total. No best-checkpoint selection replaces the
  fixed final update. Arm order is 71/original, 71/masked, 113/masked, 113/original.
- Keep the accepted PPO, replay, native limits, network, conditional port decoder,
  64-action episodes, 80/10/10 start curriculum, and CUDA execution settings.
  Only the explicit structural mask differs between paired training conditions.
- Reuse a newly created canonical fresh-input bank across arms, copying actual
  generated inputs by hardlink after each arm. Common fresh slots must have
  identical payload/geometry hashes, not merely matching generation seeds.
  Policy-dependent trajectories and inherited best-start curriculum inputs can
  differ; preserve that established curriculum instead of disabling it.
- Generate separate new development and confirmation panels, each with 16 small
  and 16 large unique factories. Exclude the earlier cold study's 64 panel
  geometries; confirmation also excludes the new development geometries. Training
  reserves all these identities before admission. No fixed factory fallback.
- Development evaluation stays at 0/32/64/96/128 with two action draws. At the
  fixed endpoint, evaluate all four trained models on the new confirmation panel
  with four action draws, both with and without masking: 1,024 endpoint episodes.
  All evaluations retain the original 64-action and 10,000 ms native allowance.

Primary pipeline comparison: **masked-trained/masked-evaluated versus
original-trained/original-evaluated**. The key additional-learning comparison is
**masked-trained/masked-evaluated versus original-trained/masked-evaluated**.
Report the remaining cross-evaluation condition without treating removal of the
mask as equivalent to its trained execution contract.

Aggregate action draws within geometry, then the two initialization seeds, with
equal size-stratum weights. Use the existing whole-geometry stratified bootstrap
(10,000 draws, seed 714600000), attaching every action/model replicate. Report
per-seed and per-size effects, geometry wins/ties/losses, development curves,
native validity, action guards, and compute/storage costs. A positive inference
screen does not predetermine the training result.

## Mask and likelihood contract

`actions.masking: "structural-bands-v1"` enables the already native-verified
structural mask. Absent/null disables it. Reject unknown modes. Share one mask
implementation between the earlier inference experiment and normal training.
Read component definitions from external catalogs at runtime; use current native
facility/component/anchor positions, including rotations.

Keep every root candidate and its index and the original per-kind normalization.
Mask only bands whose native result would necessarily be `band-cuts-body`,
`band-cuts-component`, or `band-removes-anchor`. Keep all placement, overlapping,
routing-pending, and temporarily worse preparation actions.

Masked prepared inputs carry `root_mask_schema: "declarative-root-mask-v1"`
and an aligned boolean `root_invalid` tensor. Unmasked inputs retain the existing
representation. The feature is an explicit optional extension to the internal
graph payload; model parameters, root indices and port labels do not change.
Launch/checkpoint protocol hashes bind the mode, and this study imports no
foreign replay or checkpoints.

Sampling, PPO probability ratios, successful-experience imitation, and entropy
use the same recorded source-state root mask. Never regenerate a historical mask
from a later state. Apply masking after any uniform-logit override. Store mask
tensors with the existing compact retained inputs and pack/transfer them in the
existing minibatch pipeline. Reject malformed/all-masked domains and recorded
labels selecting masked actions. A targeted native rejection in masked execution
fails explicitly. Native validation remains authoritative.

Geometry-dependent blocked coordinate sets are cached per episode revision;
immutable catalogs are reused. No route solving, score evaluation, state copying,
hashing, source replay or extra snapshot is added to the action loop.

## Continuity and readiness

| Required existing benefit | Reuse/change | Verification |
| --- | --- | --- |
| Full native draft and valid-best behavior, including preparatory edits | Existing native session and runtime; only structural band mask added | Existing preparation/live-restore tests under masking; native mask boundary audit |
| Original unmasked distribution and model initialization | Default path unchanged; no new model parameters | Exact unmasked distribution and within-seed zero-state digests |
| Packed CUDA sampling/likelihood, CPU categorical RNG, conditional ports | Add one packed boolean mask; existing policy/worker/kernel | CUDA sampled versus recomputed joint log probabilities; masked zero gradient and finite entropy |
| PPO/SIL, separate Adam ownership, KL actor stop with critic continuation | Existing optimized update kernel and recorded replay inputs | Existing nonempty-Adam replay/KL/preparation-credit tests under masking; CUDA imitation check |
| Persistent 16-worker/32-stream execution and fresh canonical inputs | Existing pool, input store/reservations, normal runner | Actual worker identities, 1,024 actions/update, fresh-slot pairing and disjoint panels |
| 80/10/10 start curriculum and complete recovery | Existing plan/checkpoint/publication mechanism | Normal masked run paused at update 2 and resumed to 4; native live-restore test |
| Compact selected replay storage, cached prepared replay, one-pass inflation | Existing store and replay compactor, with mask field retained | Save/load likelihood equality; nonempty replay in integrated readiness; measured publication/retained bytes |
| No expensive diagnostics during normal training | Dedicated bounded tests and readiness, then normal runner | Inspect actual integrated readiness cost; stop diagnostic workers before scientific arms |

Readiness uses disposable cold four-update original/masked arms and no inherited
scientific state. The original runs uninterrupted; the masked arm pauses at two
updates and resumes the same checkpoint to four. Both use the same readiness
fresh bank and existing old confirmation panel only for diagnostic evaluation.
Scientific panels/inputs and every scientific initializer remain separate.

Readiness completed in 137.71 seconds with 215,740,241 distinct artifact bytes
and 23.77 GB peak process-tree RSS. Original and masked four-update arms used
55.92 and 66.10 seconds respectively; the latter includes a deliberate restart
and an extra evaluation/publication boundary. Mean collection/learner seconds
per update were 4.694/3.633 original and 3.995/3.275 masked. This short diagnostic
establishes affordable integrated execution, not a throughput advantage.
Both exercised CUDA and successful replay (2,964/3,212 labels), all completed
native episodes validated, and 64 saved live streams restored. The masked arm
resumed normally from update 2 to 4 and retained 2,128 masked replay rows.
All ten mask tests passed with the native library, including CUDA likelihood
and imitation checks. Workspace check/tests and tracked-file checks passed.
New development and confirmation panels each selected 32 cases from 48 generated
candidates, with the required 16/16 strata and geometry exclusions.
Masked native, CUDA, replay and restoration tests must pass before readiness;
readiness must show real learning, successful replay use, complete valid native
episodes, and reasonable collection/learning/publication costs before launch.

## Resources, artifacts and recovery

The scientific run has a 9,000-second wall allowance from first launch, including
endpoint evaluation. Each arm has at most 2,700 seconds, with the existing finish
grace, and the normal 48 GiB process-tree RSS and 16 GiB free-disk limits. Each
arm's retained training files are capped at 2 GiB; the study caps actual distinct
artifact bytes at 12 GiB, checked at arm boundaries. Panel generation and
readiness each have a separate 600-second ceiling. Existing runtime evidence
suggests roughly 1.5–2 hours for the four scientific arms; readiness refines this.

Use a new isolated source/output root, reuse the pinned runtime and authenticated
release binaries, and leave the earlier study and paused main untouched. Bind
the actual code/config, release library, panels and empty-origin receipts before
training. Resume an interrupted arm from its normal checkpoint under the same
launch; never restart completed arms or reset the study clock silently.

Keep one study state record plus normal per-arm state/checkpoints and final
reports. Synchronize completed fresh input slots at arm boundaries and account
for hardlinks once. Keep diagnostic audits separate and bounded. Preserve useful
final native witnesses and non-replayable checkpoints; retain concise findings,
configuration, reproduction commands and provenance in Git, with raw artifacts
under ignored `artifacts/`.
