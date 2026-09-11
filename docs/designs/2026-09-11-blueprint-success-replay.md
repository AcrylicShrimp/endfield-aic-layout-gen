# Blueprint PPO with an accumulating self-imitation buffer

## Status and question

Registered before implementation on 2026-09-11. The user approved retaining rare
policy-discovered successes and repeatedly learning from them. Does an initially
empty success buffer improve autonomous compaction compared with PPO alone?
This is an auxiliary imitation objective, not importance-corrected off-policy PPO
or an implementation of the full published SIL algorithm.

## Frozen comparison

Use the existing graph actor/critic, kind-balanced candidates, original topology,
native validation and area-only reward. Use two fresh model seeds, 17 and 29,
128 updates per seed, eight streams, 32-action chunks and 256-action episodes.
Each arm receives 65,536 on-policy actions. Compare auxiliary coefficients 0 and
.05; other PPO settings stay at the existing values, including learning rate 3e-4.
No external demonstrations, previous run experience, teacher routing, or evaluation
samples enter either training buffer. Each seed has its own initially empty buffer.

Only original and overlap starts are used, equally represented. There is no
repair-ready or already-compacted training prefix. Evaluation uses the same two
starts, three fixed sampling streams, two seconds of native session budget, and
checkpoints 0/32/64/128. This differs from the earlier BC curriculum; compare the
two fresh arms, not absolute means from the previous three-stratum evaluation.
Retain action milestones 8/32/64/128/256 and all actual elapsed times. Per-arm
limits: 1800 seconds, 3 GiB output. An exhausted guard preserves evidence and fails
explicitly; no silent extension or retuning.

## Buffer input and storage contract

Inputs are actual authenticated training samples: observation, candidate/index,
action, reward, outcome, successor, stream/episode and chunk identity. Maintain
one open edit segment per stream across chunk boundaries. Start at the episode
boundary or immediately after an accepted restore or successful commit. Close an
edit segment at each successful commit; clear it after restore or real termination.
Archive every positive-area commit with its full segment references. Never join different episodes or skip a chunk.

Admit every segment ending in a native-validated positive-area commit, including
a routing-only improvement if one occurs. Select labels from accepted poses, repairs that
reduce pending obligations or change physical paths, and the final commit. Rejects,
physical no-ops and other intermediate actions remain in the source trace but are
not imitation labels. The selected rows retain their actual original observations;
they are not represented as a shortened, newly executable trajectory. A successful
segment does not prove every selected action was causally necessary or optimal.

Deduplicate sequence/return pairs using the blueprint identity, physical pre-action
drafts, retained-best metrics, selected actions and Monte Carlo returns (rounded
to 12 decimals for identity), excluding clock/counter features only
for deduplication. Retain occurrence/admission records for duplicates. Sample unique
sequences uniformly, not in proportion to repeated discoveries. Never evict a unique
admitted sequence during this bounded screen. Fail with retained state if a seed
exceeds 4096 unique sequences or 32768 stored label references.

Store immutable `(update, sample_index)` references into the retained authenticated
chunks, not duplicate observation payloads in every checkpoint. Buffer checkpoints
include unique entries, duplicate counts, open segments, stream episode/counter
continuity and usage counts. Chunk/state hashes bind referenced samples. A resolver
must verify older chunk references before loading them; current-chunk references
become durable atomically with the chunk/checkpoint publication.

## Learner contract

After the user supplied the classical SIL equations, the registration was amended
before implementation or execution: add positive Monte Carlo advantage weighting.
Archive a positive commit immediately, but admit its segment only after its real
256-action episode ends. Compute each label's actual discounted return through
that terminal from all recorded rewards, including rewards after the segment's
commit. This matches the existing critic's remaining-episode return semantics;
comparing a short segment return with that critic would mix different horizons.
For update n, freeze the buffer containing completed episodes through update n-1. Empty
buffer or zero coefficient must reproduce the original PPO update exactly. Sample
eight sequences per actor-active minibatch with replacement using an independent
seed/update RNG. Weight each label by `max(R - stop_gradient(V_current(s)), 0)`;
use raw native reward units without advantage normalization or a weight floor.
Within a sequence, average weighted negative log probabilities separately for pose,
repair and commit labels, then equally weight the present stages and sequences.
This bounds the contribution of sequence length and balances movement
against completion. Use the same action distribution as inference.

Add `.05 * imitation_loss` to the existing PPO loss. Keep sampling uniform; do not
add prioritized replay or a SIL value loss in this experiment. Report positive
weight fraction and complete gating so critic overestimation is observable. These labels never enter PPO
ratios, GAE, rewards or critic targets. BC reaches the shared encoder/actor without
a direct critic-head gradient. KL stopping also stops imitation and freezes actor
Adam state; the critic-head tail remains unchanged. Ingest the current chunk only
after its learner update, making every replay source strictly older than the
update using it. Record sampled IDs/row references, loss, label counts, age and
buffer hashes before/after ingestion. No updates occur from evaluation experience.

## Verification and acceptance

Test empty/zero parity, cross-chunk successful segment retention, true episode and
restore boundaries, label selection, duplicate handling, guard failures, next-update
eligibility, gradient ownership, KL freeze, and exact reconstruction from nonempty
Adam plus a nonempty buffer. Exercise actual CLI resume on a copied readiness run.
A mechanical readiness fixture may script a success solely to test the buffer;
those rows must never enter the main runs.

Reconstruct every main learner update and buffer transition from retained training
chunks, including checkpoint open segments and usage counts. Independently validate
all archived success witnesses, sampled commits and final bests. Recompute evaluation
probabilities. Reports include time/action quality, movement share, area improvements,
first admitted success, buffer growth/reuse/age, discovered versus supplied gains,
KL/compute, native/learner audits and termination. No discovered success is a valid
result: retaining experience cannot create the first success.

Versioned DTOs use `blueprint-success-*` schema names. Failures include stable
`blueprint.success-buffer`, `blueprint.success-update`, `blueprint.success-run` and
`blueprint.success-audit` identifiers with the failing entity where available.
Keep historical frozen runtime files intact and provide an explicit experimental
entry point. Generated evidence belongs under ignored `artifacts/`; commit source,
external configuration, concise findings and compact provenance. This learning
experiment does not alter the constructive-planner contracts in
`docs/designs/2026-09-02.00-constructive-layout-planner.md`.
