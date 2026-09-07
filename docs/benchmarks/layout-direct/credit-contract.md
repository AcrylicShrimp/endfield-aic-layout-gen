# Quality-first temporal credit experiment

Status: specified after the first streaming run; no effectiveness claim.

The first run provides nearly uniform operation choices even after 65,536 actions. Seven of
eight target groups receive no task signal, and raw constructor layouts give every control large
easy compaction gains. This slice changes credit assignment on the main dynamic training path.
It is not another operator capability enumeration or production integration.

## Input, output and invariant

Input: equal-length independently sampled trajectories from one complete fresh factory, each
with pre-action best score, performed-action ledger, exact accepted-state hashes and final best
score. Keep four trajectories per group. Selected actions that arrive after episode termination
are charged inference work but are not falsely recorded as executed environment transitions.

For the `quality-futility-prefix` experiment, compare terminal tuples lexicographically:
`[best area, best physical tiles, best turns, futile proposal count]`. The fourth field only
breaks exact layout-quality ties. Futile means rejected, unchanged, or an exact visited-state
return under the existing metric. A valid temporary deterioration is not automatically futile.
The retained witness still uses only the original three score fields.

Let `rank_i(v)` be trajectory i's normalized peer tournament score, in [-1, 1], where lower
tuple is better. The terminal objective is that tournament rank. For the action at time t,
use `rank_i(final tuples) - rank_i(pre-action tuples at t)` as its policy-gradient signal.
The subtracted term uses only histories before the current actions and is an action-independent
baseline; it is not new oracle knowledge. Equivalently, rank increments telescope into this
undiscounted return-to-go. Do not weight area against routing, penalize valid-worse actions, or
reward invalid retained geometry. The terminal-only three-field mode remains an explicit
experimental control, not a compatibility adapter.

Output: exact terminal outcomes, terminal ranks, nonzero per-action credit count, separated
inference/masking and update time, and all existing episode, generation, original-coverage and
independent-validation ledgers. Report quality-only rank variation separately from new tie-break
variation. Python's incremental futility ledger must equal the worker's existing summary metric.

## Failure modes and measurement

Abort with a structured training failure on unequal/truncated training trajectories, mismatched
futility accounting, changed direct-action replay, failed independent audit, or nonfinite
gradients. Saved completed groups remain evidence; do not silently replace a difficult root.

Use training schema 2 with an explicit `credit` field. The first experiment uses longer actual
512-step training, not just longer evaluation. Evaluate the exact initial, uniform and trained
policies on newly generated target-family development factories at 1,024 proposals. Keep
sampling unchanged in this credit slice so its small-factory bias stays explicit. Broader rate
sampling is a separate subsequent main-path change. Repeat model seeds if the first result is
promising before claiming effectiveness.

The first configuration also increases Adam's learning rate from 0.001 to 0.003 and trains
64 groups of 512 actions per worker (131,072 total), versus the first run's 256-step trajectories.
It measures this combined training configuration, not an isolated causal effect of the baseline
or tie-break alone. Same-seed generation requests stay replayable; constructor deadline effects
can still change acceptance and must be reported.

This objective may favor many novel but unhelpful port changes when all retained qualities tie.
Lower futile rate alone therefore does not establish learning useful packing or pass any density
gate. Inspect final quality and operation mix alongside waste. Prefix rank is a variance-reduction
baseline, not a proof of faster learning; compare actual results.

## Versioning

The original schema-1 source/config are preserved by commit `8d317f5` and the first run's source
hashes. Current training config is schema 2; old versions fail explicitly. No game or geometry
semantics change. Evaluation now retains measured decoding/masking time per worker (and batch
total), which was included only in co-batched wall time in the first report.
