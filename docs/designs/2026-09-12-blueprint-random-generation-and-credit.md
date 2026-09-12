# Restore fresh blueprint training inputs, then compare advantage scaling

Status: fresh-input restoration verified and committed as `6e7a3e5`;
advantage comparison implementation in progress. Authorized on 2026-09-12.

## Regression and required order

The user had required fresh production products, rates and initial geometry per
episode. The blueprint learner introduced in `96ab815` instead reused the
five-facility contract fixture. Continuing that diagnostic input into duration
training and the capacity screen violated the intended training distribution.
The capacity results remain observations of that fixed-fixture experiment only.

First restore and verify fresh random generation. Only after that milestone
passes, compare centered-and-standardized GAE against centered-only GAE. Keep
the blueprint semantics, reward, architecture and remaining PPO settings fixed.
No causal claim that input restoration alone fixes learning is established yet.

## Training input contract

An explicit training-input DTO selects `fresh-random` for normal duration
training. `fixed-fixture-diagnostic` exists only for named mechanical/contract
checks. Missing input declarations must fail validation rather than silently
selecting the crossing fixture. Historical runs require their frozen sources.

Fresh input configuration identifies the external random-generation spec, a
durable input directory, a seed base and a bounded admission-attempt count.
The default `@run` directory is resolved to that new run's private input store
at preparation. Sharing actual slots across experimental arms requires an
explicit shared directory; ordinary new runs do not inherit another run's bank.
Use the existing generator's four product families, rate ranges, constructive
planner and randomized valid geometry setup. Do not introduce a simpler family,
fixed geometry fallback, reward-based rejection, or an inherited best-state
curriculum. Generation is setup work, accounted separately from 64 policy actions.

Every `(stream, episode)` has a unique slot and nonoverlapping generation seeds.
Every reset consumes its next slot, including after process restoration. A
failed construction is retained with its stage/diagnostic and may use the next
registered attempt; exhaustion is a structured failure, never a fixed fallback.
Identical draws or geometries may occur naturally and are reported, not filtered
out to manufacture diversity. Seeds alone do not establish input identity.

An accepted input record binds its slot, draw, request, complete randomized
initial witness, original production obligations and native admission. It is
published atomically with file hashes. A paired experiment can share the exact
record for the same slot across policies; no slot is recycled across episodes.
Coordinate publication under a file lock and reject incomplete/tampered records.
Retain all actual accepted inputs and failure metadata. Remove regenerable
construction HTML and unselected intermediates only after retaining evidence.

Each live stream owns its request and original factory. Preserve those exact
objects and its input identity in native/learner checkpoints and chunk evidence.
Changing another stream or performing evaluation must not replace them.
Restoration revalidates bindings and restores the unfinished draft without
generation. Every commit/final witness is audited against that episode's own
original blueprint and immutable request. Native validation remains mandatory.

Normal training cannot use the evaluation cohort as an input supply. Preserve
the authentic historical 32 evaluation factories and sampling seeds, evaluate
at initialization and every four updates, and retain the existing 64-action /
two-second evaluation accounting. Measure any elapsed-guard truncation explicitly.

Diagnostics identify generation, admission, input authentication and restoration
separately. Report unique seeds, products, rates, request and geometry hashes,
admission failures, setup cost, episode coverage and retained-valid-best quality.

## Verification gate

Verify different episodes and concurrent streams obtain independent input
identities; accepted real inputs span multiple products, rates and geometries.
Verify generation occurs only at true reset, never at a 32-action chunk boundary.
Interrupt/restore a live learner and compare exact input, draft, RNG and Adam
identities; reject swapped/tampered input references. Check each episode's final
witness independently. Evaluation must neither consume generation slots nor
alter training RNG/input state. Run the required workspace and artifact checks.

## Subsequent advantage experiment

Register the concrete resource budget after measuring restored-generator
readiness, before starting either main arm. Use width 40/two message rounds,
fresh learner seeds 17/29/41, empty Adam and success buffers, eight streams,
32-action chunks and 64-action episodes. Success replay stays at coefficient .05.
Compare `(GAE - mean) / max(std, 1e-8)` with `GAE - mean`; value targets are
identical given the same recorded rollout. Do not alter learning rate, reward,
entropy, gamma, lambda, KL stopping or loss ownership.

Pair the actual generated input records and action budgets across arms. Compare
final and predeclared late-window held-out validated area reduction, own-initial
delta, success commits, action distribution, raw advantage scale and critic
error by seed. Movement frequency alone is not a winning criterion. Preserve
source/input/native provenance, exact learner replay evidence, W&B records and
resource termination reasons. No peak-checkpoint selection or automatic long run.

### Registered main budget after restoration readiness

The restored native readiness completed 24 genuine 64-action random episodes,
six exact update replays and 96 full-action historical evaluations. Register
`blueprint-advantage-screen.json` before launching main training: 512 updates per
arm, six arms, 131,072 actions and 2,048 episodes per arm, 786,432 actions total.
All arms use the same ordered 2,048 newly generated episode inputs; sharing is
across policies only. No input is cycled within a learner. Seeds 17/29/41 are
learner/sampling seeds, not six different training-data distributions.

Evaluate at 0, 4, ..., 512 (129 points per arm), using the unchanged 32 factories.
Primary outcomes are final evaluation and the last-128-update window, points
388 through 512. Retain the full per-seed results and own-initial deltas. Each arm
has a three-hour operational ceiling plus 180 seconds finalization, an eight-GiB
output ceiling and an eight-GiB shared filesystem reserve. Resource termination
before update 512 is incomplete, not a matched result. Use at most four explicitly
pinned single-thread CPU workers on one host. Source/native identity and actual
CPU allocation are recorded; elapsed policy input remains a measured limitation.
