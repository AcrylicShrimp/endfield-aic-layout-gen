# Blueprint-preserving multi-action editing

## Status and purpose

Accepted for a bounded experiment on 2026-09-11. The first native contract,
graph-input smoke check and scripted verification are complete; see the
[results](../reports/2026-09-11-blueprint-edit-contract.md).
The user authorized temporary facility overlap, unfinished
connections, and an immutable original blueprint supplied to the policy.

This experiment starts from a complete validated factory. It is an improvement
transaction spanning multiple actions, not a constructor or a production
fallback. Production construction still follows
[the constructive planner](2026-09-02.00-constructive-layout-planner.md).
Only a complete, validated, better witness can replace the retained best.

## Inputs and immutable meaning

The new `blueprint-edit-v1` session receives external catalogs, a
`FactoryEditRequest`, its complete initial `WitnessProbeInput`, and explicit
action, elapsed-time and routing-work guards. Admission validates the initial
factory before deriving the blueprint. These diagnostic resource guards are not
game dimensions or production feasibility limits.

The blueprint freezes facility IDs, catalog definitions, recipes, utilizations,
network IDs, requirement IDs, items, transport kinds, terminal IDs, exact port
assignments, directions, exact rates, external obligations, and the directed
incidence of chains at original terminals and logistics components. In particular,
equal input/output totals do not authorize rewiring. Split/merge incidence and
bridge channel directions must survive. Degree-two route cells are geometry;
their coordinates and count need not survive.

V1 moves facilities and repairs original chains. Logistics components and external
anchors keep their original positions and rotations. Unsupported ambiguous anchor
configurations fail admission explicitly; they are never silently simplified.
Component relocation and port reassignment require a later explicit contract.

## Draft, best and actions

The private draft owns facility poses and an optional path for each original
edge. It is never presented as a validated factory. Moving a facility may overlap
another facility and detach incident or newly obstructed paths. The original edge
remains present with status `pending`; it is not deleted from the blueprint.

Actions are `pose`, `route`, `repair`, `repair_all`, `commit`, and `restore_best`.
They identify original entities; none can write endpoints, rates, recipe choices,
network membership or component definitions. `route` supplies only geometry for
an original edge. Its endpoints, direction, obstacles, simple-path geometry and
intersections are checked before mutation. Native bounded A* implements `repair`,
pipe first for `repair_all`. Failure preserves existing paths and reports its
edge and exhausted work. No exact solver is involved.

Each envelope binds the blueprint hash and draft revision. Rejections consume an
attempt but do not change draft geometry, revision or best. Successful edits
advance revision. All attempted work shares the elapsed and action guards.

`commit` requires no conflicts, no pending edges, exact original topology by
construction and path preflight, and independent full native factory validation.
It updates best only on a lexicographic improvement: area, transport tiles, turns.
The reward is the fractional decrease in best validated area; draft area reduction
and repairing a previously retained best earn no reward. No overlap penalty or
repair shaping is introduced in the first experiment.

`finish` returns the retained validated best even when the draft is unfinished.
Deadline exhaustion prohibits subsequent mutation. The session has no special
32-action terminal boundary. Learning chunk boundaries must not reset the draft
or masquerade as environment termination; a future learner must bootstrap across
nonterminal chunks and preserve unfinished transactions.

The diagnostic `validated_draft_json` export is available only after successful
native admission, commit or restore-best. An unvalidated draft cannot be exported
through that witness interface. The ordinary final-result interface always
returns best, including when a valid but worse draft was committed for exploration.

## Observation contract

`blueprint-edit-observation-v1` contains the immutable blueprint and its hash,
current facility entities (including every overlapping entity), current anchor
positions, per-edge pending/routed state and paths, explicit conflicts, best
metrics, revision, and resource progress. The blueprint and draft are separate
inputs, not a raster in which the last facility overwrites earlier occupants.
An experimental graph input/actor smoke check must consume the original graph
and current geometry together. This is a new input contract; existing PPO
checkpoints must not be relabeled as compatible.

The implemented `blueprint-edit-graph-v1` input has separate static/state node and
edge tensors, explicit directed incidence, facility-to-port links, all retained
path-cell coordinates with original edge ownership, best-score context, conflict
and pending-edge summaries, and resource progress. A two-round, width-40 graph
actor/critic consumes these inputs. Its categorical fingerprints are approximate
features, never topology IDs or validator authority. Smoke candidates include
local translations, other facility origins (overlap allowed), individual repairs,
repair-all, commit and restore-best. Manual native actions additionally support
catalog rotations and checked explicit paths. This candidate generator is an
experimental subset, not a completeness claim.

## Diagnostics and first acceptance experiment

Diagnostics carry code, severity, stage, message and entity. Session-owned
diagnostics use `blueprint-edit`; shared decomposition/routing diagnostics retain
their more specific stage identifiers.
Admission, stale envelope, invalid pose, wrong endpoint, unintended intersection,
routing exhaustion, unfinished commit, native validation failure and budget
exhaustion are distinguishable outcomes.

The first slice must demonstrate: overlap survives more than one action; clearing
it allows exact original connections to be repaired; wrong endpoint geometry and
attempted topology edits are rejected before mutation; a failed repair/commit
retains best; an unfinished stop returns a valid factory; graph inputs retain
overlapping identities and original obligations; and a session can continue past
32 actions. Include branch/merge/bridge topology evidence, not only aggregate
terminal totals. Scripted success establishes the environment contract, not that
a trained policy has learned long-horizon rearrangement. PPO learning and
time-budget quality comparisons follow only after this contract is verified.
