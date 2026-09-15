# Learning I/O audit and replacement contract proposal

Date: 2026-09-15. Status: current-code audit complete for the boundaries below;
replacement architecture proposed, not finalized or implemented. Scope: native
state, observations, candidate selection, execution, reward, retained learning
records, evaluation identity and checkpoint compatibility. This is not an
exhaustive repository correctness certification or a new training experiment.

## Objective and correction

Optimize the physical factory irrespective of its global location. Translation
must preserve physical observations, corresponding available actions and bounded
execution outcomes, as well as scores. One grid cell remains physically meaningful;
scaling a factory is not a symmetry. Neither rotations nor reflections are assumed
equivalent without checking catalog and endpoint restrictions.

The earlier conversational claim that PPO rewards origin-anchored area was wrong.
`aic-data/src/layouts/constructive/factory_edit/validation.rs` calls `canonical`
before `metrics::measure`, shifting bodies, networks, terminals and components.
The existing `full_shared_graph_accepts_and_translation_does_not_manufacture_area_gain`
test was rerun and passed: the fixture keeps width 35, height 20 and area 700
under positive, negative and extreme signed translations. There is no evidence
here that the reported area scores are invalid. The observation and action
problems below are distinct from the already normalized scoring path.

All source paths below are relative to `crates/`. Findings describe the code at
`539ce89`; the pending ignored extraction-reference test does not change it.

## Confirmed mismatches

| Boundary | Evidence | Consequence |
| --- | --- | --- |
| Geometry to observation | `aic-pilot-native/src/full_factory/blueprint/declarative/learning/input.rs:26` divides by `max(1, max_x+1, max_y+1)` without subtracting minima; facility/node/path coordinates and band coordinates use that scale | Translation changes both positions and apparent object sizes. Port coordinates are facility-local, but their divisor also changes with world location. |
| Geometry to action domain | The same file, lines 204–218, rejects placement destinations with negative x or y | Moving an otherwise identical state away from the origin can add relative moves. Trimming only numerical features would leave this leak. |
| State to routing constraints | Input lines 162–163 expose original edge `first/last`; `declarative.rs::routing_blueprint` clears restrictions on changed ports, and `blueprint/topology.rs::directions` rotates or derives the effective directions | The feature columns do not directly describe the restrictions execution currently applies. Whether the network could infer every distinction from other columns has not been demonstrated. |
| Conditional action history | `aic-learning/src/collection/batch.rs:141` averages previous selected port features and retains used-port names only for masking | History loses the binding between each previous terminal and its selected port. The executor still receives the full mapping; information is lost at the model interface. |
| Physical identity to evaluation reservation | `aic-pilot-native/src/full_factory/generation.rs:52` removes rates/utilizations and sorts JSON arrays, but keeps coordinates and names | Translated copies receive different geometry identities. Source admission, curriculum and evaluation use this identity, so the reservation mechanism does not enforce translation equivalence. Actual cross-split contamination has not been established. |
| Physical semantics to numeric features | Recipe/item/port IDs become four-byte fingerprint columns in `learning/input.rs` | Renaming opaque identifiers changes learned input without changing physics. Removing them must retain commodity equality and graph incidence. |
| Execution resource to observation | `learning/input.rs:382` includes elapsed wall time | Hardware load, batching and scheduling alter the policy input for the same physical state and decision/work budget. This is observable resource state, but is inappropriate for a hardware-independent physical representation. |

The coordinate issue was also reproduced using the leaf formulas on the retained
`docs/benchmarks/constructive-bound-interconnect/fixtures/crossing-witness.json`:
translation by (100, 200) changes scale from 35 to 222 and a three-cell facility
width from 0.085714 to 0.013514. Reproducing the current JSON identity algorithm
changes its digest from
`57b0d87718bc75ca5e29ad812ee2779ebc74ec19b74ac3dedd6d527376177051`
to `6cadbabfd7aadf9bb0ffb7839fedf6d280a9031c425907c6a3083b7eefc73224`.
These are formula/identity reproductions, not model-inference measurements.

Two minimal structural counterexamples clarify the other losses:

- For one placement at (0, 0), steps {-2, -1, 1, 2} on each axis plus staying
  yield five nonnegative destinations. Translating it to (100, 200) yields nine.
  This tests the candidate formula, not a complete admitted factory episode.
- Suppose two terminals currently use c and d, and both can choose unused a and
  b. Prefixes `(terminal_0 -> a, terminal_1 -> b)` and the swapped assignment have
  the same mean port features and used set. For a subsequent terminal they supply
  the same history and mask despite different demand-to-position bindings. This
  is a representational counterexample, not a measured training loss.

## Additional risks and implicit choices

`blueprint/geometry.rs:178` clips the repair region at world coordinate zero and
1,000,000. Thus bounded search space can depend on global translation. The
formula is confirmed; this audit did not run paired full repairs to quantify it.
Retain span, materialization and work limits, but express them in the local frame.

The graph marks component presence but does not explicitly expose all physical
component types, capacities and channel-owner relations. Coincident bridge
channels must remain distinct and disconnected. A replacement needs a field-by-
field sufficiency check against the editor, not a claim that current sparsity has
already caused invalid execution.

`input.rs:359` counts all roots by family, including masked ones, while
`aic-learning/src/model.rs:367` subtracts those log counts in the distribution.
At equal remaining logits, family mass is proportional to available/total count.
With totals (100,100,100) and available counts (100,10,1), probabilities are about
(90.09%,9.01%,0.90%). This is an implicit prior, not proof of faulty PPO math.
The replacement should specify the intended prior explicitly.

`aic-learning/src/training/run.rs:72` binds configuration, source, catalogs,
panel and device, but has no explicit observation/action/reward semantics
version. A future same-shaped feature change could pass shape checks while
changing meaning. This is a migration risk; no such resume was performed here.

## Replacement proposal

```text
factory / procedural admission
    -> shared compiled physical problem + mutable draft
    -> one revision-bound physical view and candidate domain
    -> packed observation + complete action selection
    -> native bounded editor -> typed observed outcome
    -> PPO / pretraining records and ordinary checkpoints
```

1. **One coordinate authority.** Define the occupied bounding box from all bodies,
   component footprints, terminal/anchor arms and retained route cells, including
   unfinished drafts. Use signed integer coordinates relative to its minimum.
   Destination proposals may extend left/below that box; there is no artificial
   wall at the frame origin. Repair padding is symmetric in this local frame.
   Rebase after edits without changing relative positions, port assignments or
   path incidence. Keep wide-integer overflow checks and explicit span/cell/work
   limits. Export may restore a declared origin; it never enters the policy.

2. **Physical information with explicit units.** Build geometry, directed demand,
   commodity equivalence, transport, ownership, channel incidence, selected ports,
   effective endpoint restrictions, unresolved obligations and conflicts from the
   same native authorities execution uses. Normalize world positions after origin
   subtraction; leave facility-local port offsets local. Preserve grid scale with
   span/cell-unit metadata. Encode direction categorically or as vectors. Use
   catalog properties and relations instead of numeric ID hashes. Keep best and
   initial score context, remaining decisions and actual routing-work allowance.
   Wall deadlines remain operational abort controls with censored outcomes.

3. **Action selection preserves bindings.** Keep placement with joint rotation/
   terminal-to-port assignment and band insertion/deletion. Each prefix entry
   pairs its terminal embedding with its chosen port representation before any
   learned aggregation; do the same for a complete pretraining action. Keep exact
   typed assignments outside the neural summary for execution and learning
   records. Preserve the conditional without-replacement masks and joint log
   probability. As a minimal prior correction, count available roots per family
   so equal logits give equal available-family mass; record this as an explicit
   changed policy contract. A separate family head is not required by this proposal.

4. **Revision-bound output.** A selection refers to its retained native domain and
   revision and resolves to exact integer geometry and assignments. No float-to-
   coordinate reconstruction. Reject stale/foreign handles and malformed bindings.
   Return changed/unchanged/rejected, pending/conflict state, charged work, detailed
   stop reason, current accepted metrics when available, and retained-best metrics
   separately. Preserve current scoring and terminal credit. Failed drafts never
   receive invented area labels or become claims of global infeasibility.

5. **Separate identity purposes.** Keep exact source identity for loading and
   recovery. Define a versioned, translation- and opaque-renaming-invariant
   structural reservation key using typed physical primitives and incidence.
   Preserve ordered path/direction semantics; do not recursively sort arbitrary
   arrays. Keep a separate exact physical problem identity including rates and
   catalog constraints. Root lineage reserves all descendants in one split even
   when their geometry changes. Compute these identities at admission or explicit
   publication, not per action. Ambiguous structural canonicalization requires a
   bounded matching decision; it must not silently certify split separation.

6. **Explicit artifact cutover.** Bind physical schema, observation semantics,
   candidate/mask/prior rules, action encoding, reward semantics and encoder shape
   in dataset/model/checkpoint metadata. Replace the production I/O path fully.
   Retain old reports and artifacts as historical evidence; old learned tensors,
   moments and feature replay must fail clearly at the new semantics boundary.
   Compatible physical snapshots may be admitted through a documented cold
   conversion, preserving actual draft/best state and obligations. This does not
   claim continued optimizer training or exact old-policy replay. Do not maintain
   two production feature builders just to keep old tensor widths usable.

This supersedes the earlier proposal to zero some columns while retaining the
24/14/7/12/12 widths, old normalization and parallel legacy feature profiles.
The pretraining objective, shared native editor and paired transfer experiment
remain useful. The exact field schema, structural-key canonicalization and
checkpoint migration contract remain open design work before implementation.

## Continuity and adoption checks

| Existing benefit | Required preservation and bounded check |
| --- | --- |
| Correct area and valid-best scoring | Keep full canonical validation and lexicographic metrics; retain translation regression |
| Fixed obligations and exact execution | Keep all directed rates, external terminals, bridge-channel ownership, complete port maps and stale-handle checks; compare typed round trips |
| Transactional repair and unfinished drafts | Keep pipe-first shared budgets, pending recovery, accepted best and no-work unchanged edits; compare corresponding scripted actions |
| Owned history and PPO credit | Keep owned pre-action arrays, frozen masks/branches, joint log probability, true-terminal GAE with chunk bootstrap and complete-episode positive replay |
| Efficient native collection and learning | Reuse persistent workers, bounded queues, packed graph/path inference, optimized masked references and foreach Adam; measure an integrated release workload |
| Validation and recovery cost | Validate stable inputs once, reuse revision-bound views, retain existing atomic checkpoints and actual-state restore; no per-step hashing, replay or duplicate snapshots |
| Evaluation separation | Reserve normalized structural identity and root lineage across all sources; translated/renamed/descendant admission checks |

The first implementation gate should exercise the entire boundary on a small
factory and procedural set, before pretraining collection:

- Translate complete and pending states; align row/action identities and compare
  observations, domains, masks, selected actions, repair outcomes, work and scores.
- Rename and reorder opaque entities while retaining all relations; require the
  corresponding model/action permutation and unchanged reservation identity.
- Swap prefix bindings; verify the terminal-to-port relation survives encoding.
  Change rotation/ports and compare observed restrictions with execution authority.
- Pack multiple graphs, freeze a branch, mutate another live state and restore a
  pending snapshot; check ownership, joint log probability and exact obligations.
- Reject incompatible artifacts and censored resource exits explicitly. Compare
  release collection, packing, storage, validation and learner costs with retained
  evidence; document intentional domain changes separately from regressions.

These are adoption checks, not audits to run inside every normal step. No new
training run, replacement implementation, or performance benefit is claimed by
this document. The pending extraction measurement remains separate paused work.
