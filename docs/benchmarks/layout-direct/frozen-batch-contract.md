# Independent frozen-policy batch pilot

Status: preregistered before collection, 2026-09-07. No area-reward redesign.

## Question and population

Does the existing task estimator produce consistent directions across independently sampled
groups, and do its one-step updates improve separate matched rollouts? This is a local
learning-response pilot on two explicit complete factories, not arbitrary-factory validation.

Freeze the initial width-32 policy and empty Adam snapshot from update-audit-02 capsule 1
for **every** root, batch and arm. Preserve its learning rate 0.003 and norm-1 clipping;
disable entropy throughout to isolate task signal. No accumulated training between batches.
Pin and verify both original capsules using the hashes in `update-replay-report.md`.

Roots: (1) the complete seven-facility factory with worker 0's saved best witness from
capsule 1, score `[567,86,26]`, and (2) capsule 2's original complete 59-facility target.
The first is explicitly an already-compacted descendant of the original request, not a
cropped factory or an independently generated generalization sample. Bind the descendant
to its hashed source episode; preserve all request/graph/facility/transport semantics.
Audit roots through the real environment before collection. No constructor-policy change.

## Fixed allocation and RNG

Eight independent batches per root; four 128-action behavior trajectories per batch.
For each batch evaluate four arms, each on four separate 128-action trajectories:

1. Current quality/futility prefix credit.
2. Terminal ranks of the identical final quality/futility tuples, without the prefix baseline.
3. Current credit sequences cyclically reassigned `[1,2,3,0]` to workers, without entropy.
4. Zero signal and zero entropy with the same empty Adam state (unchanged policy control).

Total: 8,192 behavior proposals and 32,768 evaluation proposals, 40,960 overall; 320 final
episode audits plus root checks. No pilot-based changes to this allocation, no selection of
the best arm/checkpoint, no extra runs to obtain a favorable result. Fail rather than truncate
128-action episodes or substitute roots. Per-episode limit: 300,000 ms. At most four worker
processes. Persist all behavior capsules, arm updates and evaluation episodes; 2 GiB total
output guard and the existing 512 MiB per-capsule guard stop with structured failure.

Initialization is frozen, root order is fixed, and no generation is requested. Derive integer
seeds from SHA-256 of `["frozen-batch-v1", role, root_index, batch_index, evaluation_index]`.
Behavior uses the existing per-worker action streams. Evaluation uses four separate seeds,
one episode per call, because the existing evaluation helper intentionally uses worker index
zero. Match each evaluation seed across all four arms, but never reuse a behavior seed or
an evaluation seed from another batch. Shuffle assignment is fixed, not chosen from results.

## DTOs and invariants

CLI: `--config` with version-1 input capsule references, allocation and frozen seed namespace;
`--output-directory` must not exist. Save hashed config/source references, derived roots and
root audits. Each batch stores a standard replayable prefix-update capsule, all four exact
signal arrays, before/after snapshots, raw gradient vectors, losses and pre-clip norms,
and references to every independently audited evaluation episode. Existing `apply_update`
and `rollout` implement the real training arithmetic and executor, not a surrogate learner.
Raw diagnostic gradients must fingerprint-match the actual update. Zero arm parameters
must remain exactly identical. Replay the prefix capsule immediately; compare all source
hashes again after the run. No silently missing or invalid batch is included in aggregates.

Reports use stable code `frozen-batch-failed`, severity `error`, stage `frozen-batch-pilot`,
message and config/batch entity for errors. Completed evidence remains on failure; a partial
run is not analyzed as the predefined complete allocation. Finite exclusive JSON writes only.

## Registered endpoints and interpretation

Primary endpoint per root and arm: paired lexicographic final-best wins minus losses against
the zero arm on prospective evaluation episodes (area, physical tiles, turns in that order).
Also report paired area, tile and turn differences separately, unchanged/invalid/revisited
action rates, actual consumed proposals/route expansions, complete scores and density.
Do not turn component differences into an unapproved weighted reward or confuse action novelty
with quality. The original retained-factory ordering is unchanged.

Treat a full four-trajectory update batch, not a trajectory step, as an independent replicate.
For task gradients report per-batch norms, mean gradient norm, trace of sample covariance
(denominator B-1), pairwise and leave-one-batch-out direction cosines. A descriptive ratio of
mean-gradient norm to RMS batch dispersion is not a precise population gradient SNR estimate.
Compare prefix and same-outcome terminal on the same behavior batches; report observed
covariance trace ratio without claiming eight replicates prove general variance reduction.

For paired quality report every batch's mean over its four evaluation seeds, then the mean
and a fixed 2,000-resample batch bootstrap percentile interval (seed namespace `bootstrap`).
Eight batches and two selected roots support only a variance/response pilot, not production
significance, target density success, or broad generalization. Do not pool factories as though
they were independently sampled from the intended release population. Report all arms.

## Gate and next decision

Inspect actual geometry for any claimed packing win, including unchanged target outcomes.
Use the zero and shuffled arms to distinguish a useful response from arbitrary parameter
movement. If evidence is mixed/weak, report the uncertainty rather than declaring capacity
failure, mathematical impossibility, or success. The user's proposed absolute-area/density
reward remains a hypothesis, not an instruction to change the objective. A next intervention
must follow this evidence and the existing research gates; no automatic reward/capacity sweep.
