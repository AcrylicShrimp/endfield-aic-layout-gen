# Existing success learning diagnostic

Status: complete.

## Context and contract

The five large-factory training successes were already used by their originating
standardized PPO/SIL learners. Determine whether saved policies select those
recorded actions and whether replay actually supplied positive imitation weight.
This is a bounded, explicitly invoked diagnostic on training examples, not a
generalization evaluation or a change to normal training inputs.

Inputs: the completed six-arm advantage matrix and the retained five-entry
selection from the preceding bottleneck report. Outputs: JSON with bound input
references, per-action probabilities/ranks, value predictions, positive SIL
weights, source-chunk PPO credit, and historical replay-use attribution bounds.
Use the actual recorded tensors, action candidates and normalized time features.
Do not regenerate factories or substitute simplified successful trajectories.

First inspect checkpoints 0/128/256/384/512 for all six policies, plus the first
retained four-update checkpoint at or after each originating admission. Read
only the referenced success chunks and the two originating arms' update JSONs.
Historical minibatch diagnostics aggregate all sampled sequences: distinguish
proven positive/zero weights from mixed batches with unknown per-label weights.
Checkpoint weights are snapshots, not historical per-minibatch measurements.

Stop this first probe after the registered observations are complete. Add a
bounded follow-up only if it answers a remaining concrete ambiguity. Any needed
new training must use the established optimized training path, as the user
explicitly requires; do not introduce an ad hoc slow fit loop.

## Continuity checklist

- Preserve frozen topology, endpoints, rates, validity and retained best: no
  native actions or output replacement in the first probe.
- Preserve fresh random training and independent evaluation: recorded training
  cases are diagnostic inputs only; no evaluation cases enter fitting.
- Preserve existing model and SIL semantics: reuse packed graph distribution,
  PPO credit and return-minus-detached-value weight; keep coefficient .05.
- Preserve performance: separate one-thread process, bounded reads/inference,
  cache stable inputs and hash once; no normal-loop audits or checkpoint edits.
- Verify actual CLI execution and probability/credit identity checks; use
  existing workspace evidence proportionally and run required source checks.
- Keep raw output ignored; commit compact findings, source and provenance.

## Completion criteria

Report what was learned, what is still unidentified, whether another training
run is justified, reproducible commands and checked artifact identities. Finish
the work unit with a focused commit and worktree status.

## Activity

- 2026-09-13: inspected the originating runner, success buffer, SIL loss,
  saved checkpoint layout and recorded minibatch diagnostics. The five entries
  were admitted at updates 40, 418, 418, 128 and 296; historical use counts alone
  cannot establish their positive weights.
- The first probe completed 33 checkpoints and 15 recorded labels. At least
  90.9--97.4% of each entry's uses had positive weight on every label. To inspect
  relative loss influence without fitting, add exactly two backward-only probes
  at the saved starts of update 512 in the originating arms. Compare the full
  recorded PPO actor gradient with each selected success's expected SIL gradient
  under uniform buffer sampling, coefficient .05 and stage balancing. Report
  pre-clipping gradients and cosines as a snapshot, not accumulated optimizer
  influence. No optimizer step, rollout or fresh training is needed.
- 2026-09-13: completed 495 fixed-state predictions and two backward-only
  probes. Historical all-positive uses account for 2,451/2,596 selected sequence
  uses. Two successful pose probabilities improve and three decline; all five
  remain unlikely. Expected rare SIL gradients are 0.1590% and 0.0192% of the
  full PPO actor gradient at the two inspected late points. Shared value-loss
  encoder gradients are included; no whole-history causal claim is made.
- The final actual CLI passed its source/input/credit/model identities and
  probability checks. Workspace check/tests passed. Continuity checklist closed
  for this diagnostic: no training, native actions, input regeneration, policy
  change or optimizer update. Raw evidence is ignored and the findings are in
  [the report](../reports/2026-09-13-blueprint-success-learning-diagnostic.md).
