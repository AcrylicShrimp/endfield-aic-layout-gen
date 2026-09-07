# Reproducible on-policy update contract

Status: specified before implementation, 2026-09-07. Research gate 0; not an efficacy trial.

## Scope and frozen semantics

Instrument the existing streaming trainer, not an alternate training pipeline. Keep the same
network, sampling policy, action vocabulary/masks, random draws, credit function, sample-normalized
combined loss, gradient clipping and single Adam update. Retained factory quality rules do not
change. No constructor-policy extension is authorized. This gate must reproduce the original
update before investigating capacity, training budget or alternative credit.

The hard cutover is in the training evidence contract: each newly recorded training action has
behavior probabilities and each completed update has a replay capsule. Old incomplete records
are historical evidence only; the verifier rejects them rather than synthesizing missing data.
The training configuration remains schema 3; new capsule DTOs use their own schema 1.

## DTOs

- Decoder trace: ordered conditional stages, chosen index, exact legal support, vocabulary
  size, normalized log probabilities (null for masked entries), probabilities, selected log
  probability and conditional entropy. Diagnostics detach tensors and consume no randomness.
- Recorded history: pre-action observation, conditional decisions, emitted action, behavior
  joint log probability, mean conditional entropy and decoder trace. Preserve pre-action score,
  remaining budget and futility; do not silently replace them with terminal observations.
- Update capsule: source hashes; execution precision/thread/runtime identity; original complete
  root and episode artifact references; loss configuration; state-deduplicated histories and
  terminal summaries; computed credit; before/after model, Adam state and CPU Torch RNG state;
  update loss/gradient fingerprint and conditional distribution-change diagnostics.
- A group report references its immutable capsule by path and SHA-256. JSON writes must be
  finite, exclusive and bounded at 512 MiB per capsule; exceeding this safety guard is a
  structured evidence failure, not permission to truncate records or silently skip recording.

JSON tensor states preserve dtype and shape; mapping keys preserve integer optimizer indices.
Do not use pickle. The snapshot restores one optimizer update. It is not a resumable factory
generator/worker checkpoint: streaming cursors, deduplication and in-flight environment state
are outside this gate. Initialization/data RNG decoupling remains a separately specified change.

## Invariants and failure modes

Replay before the optimizer step must reproduce the exact action, decisions, behavior log
probability, entropy and masked conditional distributions. Detect a stale behavior snapshot
before updating. Use the original combined-backward arithmetic; diagnostic gradient collection
must not change optimization ordering or replace the task objective.

The offline verifier checks source/runtime identity, reconstructs credit and all loss inputs,
restores the optimizer including its moments, repeats the single update, and compares the entire
after snapshot and gradient fingerprint. It performs no factory search, gradient retuning or
artifact overwrite. Fail with a stable diagnostic code, stage, severity, message and capsule
reference on malformed schema, source/runtime mismatch, altered behavior, altered credit,
changed legal support, nonfinite tensors or a nonreproducing update.

Conditional KL/TV after an update is measured on recorded old-policy prefixes and fixed
observations. It is not full trajectory KL or policy quality. Report vocabulary support and
counts per decoder stage; avoid comparing raw entropy across unequal choice spaces as though
it were a normalized learning measure. Separate extra recording/replay/serialization time and
bytes from environment proposals, routing work and update time.

## Verification gate and bounded execution

1. Enumerated four-trajectory, two-step reference: compare the actual credit estimator with
   a fixed-opponent finite-difference objective; verify prefix subtraction changes neither
   expected gradient nor final objective. Include quality/futility priority and ties.
2. Decoder instrumentation on/off must preserve actions, RNG state, loss and parameter
   gradients. Cover all operation kinds and all conditional stages with real complete roots.
3. Recorded versus original combined update must match exactly. JSON restore must reproduce
   a second Adam update with nonzero prior moments, not merely its empty initial state.
4. Corrupt behavior, credit, state references and checkpoint values: reject the capsule.
5. Run a predefined main-trainer plumbing check: two groups, four workers, 32 actions each,
   seed 2901, width 32, unchanged credit/Adam settings; first request the small stratum, then
   explicit target replay. Evaluate one fresh small development root plus target at 32 steps
   with initial/uniform/trained controls. This is 256 training and 192 evaluation actions.
   The small diagnostic slot is explicit, not a claim of arbitrary-factory performance or a
   replacement for the separately failing required medium/large curriculum. No seed retries
   or selection by favorable quality. Abort rather than substitute roots on generation failure.
6. Replay both saved updates in separate processes and record capsule hashes, measured overhead,
   preserved validity and all discrepancies. Passing permits learning diagnostics, not a
   capacity/budget efficacy conclusion or production cutover.

Per-trajectory task-versus-entropy gradient variance, frozen checkpoint quality curves,
zero/shuffled-signal training and capacity/budget experiments remain subsequent gates. Do not
claim this persistence slice measures them. No historical missing record is recovered by it.
