# Human-guided completion research

Status: geometry diagnostic and research decision; no production planner changes.

## Diagnostic contract

Input: schema-1 screenshot observations in `completion-envelope.json`, and an explicit
runtime facility catalog. Footprints and allowed rotations come from that catalog, not code.
Two snapshots retain the same seven named facilities. The second is a counterfactual:
remove the final two facilities and their incident belts from the user's final screenshot.
It is not an independently observed intermediate state. Preserve all nine observed facilities;
the user acknowledged that a smaller production plan could use one fewer planter.

Output: JSON on stdout, with occupied cells, partial bounds, and the minimum enclosing area
for the two remaining bodies while retaining the snapshot's facilities and visible belt cells.
Enumerate every integer enclosing frame up to the explicit diagnostic area ceiling, including
expansion on any side, every allowed body orientation, and every integer body position.
Report the first feasible area, one placement witness, and frames/poses/pairs examined.
If no frame succeeds, report `envelope-bound-exhausted`, not factory infeasibility.

This is a geometry relaxation, not a router, optimizer, production fallback, or factory
validator. It omits new belts, port accessibility, commodity flow, external boundaries,
logistics components, and all power. A feasible packing need not be routable. A minimum is
exact only for these frozen masks and two unconnected bodies. Never prune a movable factory
state using this conditional result. The area ceiling is an experiment bound, not a game limit.
The screenshot grid is inferred, not yet user-confirmed; small numeric differences are tentative.

Failures use `envelope-input`, severity `error`, stage `completion-envelope`, message and
entity where available. Success and exhaustion also carry structured diagnostics. Reject
unknown catalog IDs, illegal poses, overlapping observations, diagonal/repeated belt cells,
duplicate identities, or unsupported inventory sizes without repairing them.

Completion criteria: deterministic replay, positive and negative CLI checks, exhaustive
toy-case cross-check, recorded results, and a research recommendation preserving routed
incremental transactions. No constructor implementation is authorized by this experiment.

Activity log:

- 2026-09-06: Recorded the user's chain-folding, spacing, port-selection, and remaining-shape
  reasoning. Began a same-inventory frozen-mask comparison before changing production code.

## Findings from the user's explanations

The useful representation is a connected chain with bends and adjustable intervals, with
branches and merges attached to it. It is not an immutable rectangular module. The battery
examples emphasize ordering upstream chains around a high-fan-in consumer; the citron powder
examples emphasize straight versus folded seed/plant chains; the enriched iron example adds
the decision to move two already considered iron chains before inserting the final consumer.
These are qualitative observations, not a claim that every later screenshot has been fully
transcribed and validated. The first battery reference has separate validation in `README.md`.

The user starts many same-size straight connections with a one-cell gap and centered ports,
then shifts selected ports one or two cells to clear useful interior space. These are proposal
preferences, not game rules or completeness-preserving pruning. High-fan-in port order follows
where the upstream chains are located; it is not a universal left-to-right facility order.
Longer connections can be useful if they create room for another facility. Empty area alone,
minimum individual belt length, and current partial bounding-box area miss that relationship.

The enriched iron decision sequence was: consider a straight seed/plant group; reject an L
whose cavity would not accommodate the iron chains; initially put two furnace/grinder chains
beside the group; anticipate the thickener and last grinder; move/rotate the iron chains below;
extend the iron connections by one cell to align the row; finish with port/pose adjustments.
The user explicitly accepts a small final protrusion. None of these statements establish
optimality of a block, a hard bounding box, or an intentional need for two planters.

## Controlled geometry result

| Frozen seven-facility state | Current bounds / area | Empty cells | Minimum with two new bodies |
| --- | --- | --- | --- |
| Rejected side-by-side state | 21x8 / 168 | 49 | 21x10 / 210 |
| Relocated state derived from final | 15x12 / 180 | 59 | 16x12 / 192 |

The two missing bodies occupy 33 cells. Thus 49 empty cells do not imply that the original
envelope can accommodate their shapes. The current-area ordering favors the rejected state;
the conditional completion-area ordering favors the relocated state. This demonstrates the
failure of that particular partial-state ranking signal on the inferred masks. It does not
prove that the current constructor actually proposes either of these two states.

The first minimum needs 36 frames, 11,134 body-pose checks and 373 pair checks; the second
needs 2 frames, 634 pose checks and 233 pair checks. All strictly smaller enclosing frames are
exhausted, across every allocation of additional space to the four sides. One shape witness
is returned, not all ties. Square rotations are geometrically deduplicated in this relaxation.

The observed final mask is 16x13 / 208, with 144 facility cells and 19 visible belt cells.
That is NOT a validated operating factory. In particular, the geometry-only 192-cell result
rotates the thickener differently and places bodies against neighboring bodies; its required
ports and routes have not been checked. It is not an improvement proposal. The difference
between 210 and the observed 208 is only two cells and is sensitive to transcription errors.
Do not inflate it into a large measured factory improvement. The stronger finding is the
ranking reversal and the insufficiency of free-cell count.

## Implementation audit

At repository baseline `5fa3683`, production
[`placement.rs`](../../../crates/aic-data/src/layouts/constructive/production/placement.rs)
shortlists nearby single-facility poses using current enclosing area and anchor distance
(`shortlist`); `select_nearby` compares validated routed partial states with the tuple
`[area, physical cells, turns]`. It protects pending port corridors, which is useful but not
an estimate of where all remaining facility bodies and their inputs will fit. This describes
the optional nearby-selection path, not a claim that every production decision uses it.

The paused, uncommitted breadth experiment also selects the best currently validated variant
and tries ready consumers by frontier depth then identity. Its current changes are not part
of this research commit and are not a stable production baseline. Neither increasing its
candidate count nor adding another free-port check directly models the user's chain relocation
and completed-envelope comparison. The inspected code supports this architectural gap;
an end-to-end quality gain from a replacement has not been measured.

The final score priorities are not the problem and must remain unchanged. A partial-state
proposal/ranking heuristic and the score of a completed validated factory are different things.

## Candidate strategies and decision

| Strategy | Contribution | Limitation / decision |
| --- | --- | --- |
| Connected-chain bend, slide, rotation and gap proposals | Generates meaningful coordinated alternatives, including deliberate slack | Keep several variants; never freeze a chain into one optimal rectangle. Recommended proposal representation. |
| Remaining-body packing / port-access estimates | Detects unusable pockets and anticipates expansion cheaply | Body packing alone ignores route feasibility. Use for ranking and diagnostics, not unconditional rejection. |
| Bounded routed completion trials | Compares consequences after adding the remaining connected work | Costs construction time and may fail under a budget. Recommended validation of promising proposals, retaining any complete incumbent. |

The rollout literature evaluates a next choice through a heuristic completion, rather than
only its immediate cost. Retaining the original complete solution provides a non-worsening
fallback. Its stronger monotonicity guarantees require assumptions such as sequential
consistency; a bounded constructor that can fail does not automatically satisfy them.
This is a methodological connection, not a measured result for this repository.
[Bertsekas, Rollout Algorithms for Discrete Optimization: A Survey, sections 2–2.1](https://web.mit.edu/dimitrib/www/Rollouts_Survey.pdf).

For a future bounded packing oracle, OR-Tools' official example uses paired coordinate
intervals and `add_no_overlap_2d`. That supports the rectangular-body subproblem, not routing,
port access or whole-factory infeasibility. This experiment uses direct enumeration and adds
no solver dependency. A future oracle would need its own local-neighborhood contract.
[OR-Tools rectangle covering example](https://github.com/google/or-tools/blob/stable/examples/python/cover_rectangle_sat.py).

Recommendation: combine chain-aware proposals with completion-aware evaluation. Do not
replace the planner with global facility-only packing followed by routing. Do not implement
all three mechanisms as one broad cutover before evaluating a small diagnostic slice.

## Proposed next experimental contract (not implemented)

Input: an immutable validated partial state, fixed production inventory and rates, remaining
dependency graph, a bounded recent connected growth window, and explicit shared work budgets.
Derive chain relationships from the graph; do not embed screenshot coordinates or recipe
specific dimensions in the proposal generator. Permit a few bent/straight forms, gap lengths,
and boundary-aware port assignments, with retained alternatives when a preferred pose fails.

Output: per-proposal transforms, affected facilities/networks, estimated envelope, packing
relaxation result, actual completion status, validated final score when available, work counts,
termination reason, and the retained witness identity. Keep `estimate`, `local geometry bound`,
and `validated complete score` separate fields. Unfinished trials report unknown, not a
fictional final score. Report whether a candidate was never proposed, filtered, unroutable,
budget-exhausted, valid-but-worse, or selected.

Trials grow privately by one connected frontier facility, atomic cycle, or already approved
downstream-closed transaction at a time, assigning ports and routing each new connection before
the next growth step. Charge proposal generation, local packing and every reroute to explicit
budgets. Pipes remain first; preserve commodity network synthesis and the dedicated router.
Only commit a validated closed growth state. Improvement trials retain the complete incumbent
unless a complete validated candidate improves area, then transport cells, then turns.
Chain-level transformations must use bounded rollback/replay or a transactional improvement
window, not mutate unrelated validated regions or freeze all facilities globally.

The first ablation should separate candidate generation from evaluation: existing proposals
with current scoring, chain-aware proposals with current scoring, then the same proposals
with routed completion trials. Match inventory, boundary semantics and total work budgets;
record candidate recall, completion rate, area/cells/turns and elapsed work. Test all three
recipe families rather than tuning only to the iron screenshot. Keep the nine-facility human
reference separate from the smaller eight-facility production plan. Before larger rollout
trials, reconstruct explicit per-facility utilization and split route rates so these examples
can be validated without assuming full utilization or duplicating a source's entire output.

This follows the incremental routed-transaction and complete-incumbent requirements in
[`2026-09-02.00-constructive-layout-planner.md`](../../designs/2026-09-02.00-constructive-layout-planner.md)
and `AGENTS.md`. No exact global solver, new propagator, global infeasibility claim, power
requirement, fixed blueprint bound, or production fallback is introduced.

## Verification and provenance

```sh
node docs/benchmarks/user-layout-reconstruction/envelope.mjs \
  --case docs/benchmarks/user-layout-reconstruction/completion-envelope.json \
  --facilities data/game/normalized/facilities.json
node --test docs/benchmarks/user-layout-reconstruction/*.test.mjs
cargo check --workspace
cargo test --workspace
```

All nine reconstruction/geometry tests pass, including five new tests covering result replay,
bound exhaustion, translation, malformed inputs, and independent brute-force agreement on
six synthetic two-obstacle configurations. CLI success and missing-file failure were checked
manually. Workspace check and tests pass (524 tests passed, two ignored). No Rust files changed.
No rendered layout or independent factory-witness validation is claimed by this new diagnostic.

SHA-256 inputs:

- Case JSON: `c296afd5d61169bef03c0d5f04469376bfe25fa0fda14af69dd76e523d5fcd4c`.
- Facility catalog: `a314f24f7220297f5d4db65d4612d76c1f7b9c78e16f5c4bb6358b3834a69a9e`.
- Rejected screenshot: `7a25ac4d5e674df12e29ec45788012ac2f077976ec973421ce59e489ebbeea9f`.
- Final screenshot: `4da64fc6a2f6ee80fc3b7a6b16c0db6d4106730f2f016093e6c4cfbaebbb93ef`.

Sources were inspected on 2026-09-06. Remaining risk: inferred screenshot grid, unvalidated
new route/port assignments and rates, only one quantitative decision example, optimistic
geometry relaxation, and unknown cost/quality of routed completion trials. Additional user
screenshots are not needed for this next experiment. Ask for a targeted example only if a
specific unresolved choice blocks it.
