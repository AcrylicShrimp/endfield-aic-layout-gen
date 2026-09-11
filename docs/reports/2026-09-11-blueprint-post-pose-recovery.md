# Recovery after actual policy moves

Both problems are present: many sampled moves require further facility movement,
and the policy also discards drafts that the existing router can already finish.
Routing work limits were not the decisive blocker in the recoveries found here.

This follows the [persistent PPO screen](2026-09-11-blueprint-persistent-ppo.md)
and its recorded-credit diagnosis. The user requested testing actual post-pose
drafts and comparing recovery with the policy's subsequent actions. The
[probe contract](../designs/2026-09-11-blueprint-post-pose-recovery.md) fixed the
population and bounded routing cascade before execution. No learner, reward,
production planner or native routing implementation was changed.

## Population and fixed-pose results

The census covered all 2,597 sampled training poses across both seeds. Of these,
130 were rejected by the native pose contract; all 2,467 accepted post-pose
states were reconstructed from authenticated observations and strictly restored
through the native checkpoint API. This includes repeated states and late
moves, not only successful trajectories. Counts are correlated pose states,
not independent episodes or a general recovery success probability.

| Result with the sampled facility poses held fixed | States |
| --- | ---: |
| Facility/component overlap: further movement required | 1,238 |
| Pending endpoint inside a facility body | 2 |
| Originally distinct connection endpoints collapsed onto one cell | 109 |
| Complete fixed-pose recovery validated | 874 |
| No path found by the bounded routing cascade | 240 |
| Routes produced, but complete external-attachment validation failed | 4 |
| **Total accepted post-pose states** | **2,467** |

The 109 collapsed connections were checked against the original blueprint:
all had `zero_length = false`. Changing the route order cannot make those
unchanged endpoints distinct. The four complete-validation failures reported
`factory.geometry-invalid` for external attachment `external:C0:output-belt-0`.
They were rejected; no invalid witness was exported.

The raw probe report groups the last three failure subtypes under 353 bounded
unresolved states; the independent summary separates their observed causes.
The 240 remaining no-path results are not proofs that the geometry is globally
unroutable. This probe preserved existing routed paths, tested finitely many
orders/windows, and did not run a global rip-up or placement search.

Of 874 validated recoveries, **842 succeeded with the original repair-all
configuration**. Another **32 succeeded with individual pending repairs in
reverse edge-ID order**, retaining pipe-first routing. Raising expansions from
100,000 to 1,000,000 and increasing padding from four to 16 did not recover any
additional state before the order change. Those are conditional cascade results,
not independent treatment-effect estimates.

The first accepted seed-17 move is an illustrative order failure: original
repair-all stopped after 334 expansions; the reversed sequence validated after
683. Its resulting score was `[770, 135, 20]`, worse than the retained
`[700, 129, 18]`. Thus successful routing alone is not a positive learning example.

## What the policy actually did

Among the 874 fixed-pose recoverable states, the next decisive event was:

| Next event before any intervening accepted pose/restore/valid commit | States |
| --- | ---: |
| Restore the retained best | 417 |
| Another accepted pose | 340 |
| Successful commit at the same poses | 112 |
| Episode termination | 5 |

A new pose is further editing, not automatically abandonment. Restore-best
explicitly discards the current draft. We followed the real 256-action episode
across chunk boundaries and counted repairs before that next event. All 112
actual same-pose completions also belonged to the independently recoverable
set; their recorded commit witnesses passed a separate audit.

More decisively, **46 post-pose states in 25 episodes had a validated recovery
with lower area than their then-retained best**:

| Policy continuation from an area-improving opportunity | States |
| --- | ---: |
| Successful same-pose commit, with positive area improvement | 4 |
| Restore-best before committing | 16 |
| Another pose before committing | 26 |

There was no rejected repair action before those boundaries. Twenty-nine of
these 46 states had no repair action at all before the boundary; 17 had at least
one. All 46 opportunities used just original-bound repair-all plus commit, and
every source state still had at least nine actions remaining in its real
episode. Their missed commits were not forced by a one-action remainder.
The 26 followed by another pose had no later positive area reward in that
real episode. These are observed trajectories, not a claim that the next pose
caused the later failure.

The opportunities were not all supplied by the repair-ready prefix: 17 came
from original starts, 12 from overlapping starts, one from already-improved
starts, and 16 from repair-ready starts. The latter 16 can still benefit from
planted geometry and must not all be described as discovered compactions.

For example, `s17-u0025-v5-a018` came from an original start. The policy moved
into a state that original repair-all plus commit could validate at area 665
versus the retained 700. Its very next action was restore-best, without a repair
attempt. This directly demonstrates a sequencing/opportunity-recognition problem,
without requiring a stronger router for that particular draft.

Of the 1,238 overlapping post-pose states, 651 introduced overlap from a
conflict-free state. Only 12 subsequently cleared all overlap before the next
restore-best or episode end. Their immediate boundaries were 646 restore-best,
581 further poses and 11 terminals. The first two counts alone do not establish
whether a particular extra move was useful; they show why overlapping recovery
needs more than an individual route-repair action.

## Verification and interpretation

The main probe completed in 66.37 seconds. Each diagnostic branch restarted the
same geometry with a fresh five-second clock and 64-action allowance. Original
or expanded routing budgets were explicit, and ordered branches shared one total
expansion allowance across their calls. These are counterfactual resources,
not the actual source policy's remaining action/time budget.

The probe checked 453 unique counterfactual witnesses. The separate summary
audit restored all 2,467 source states again, checked every retained case and
source chunk hash, recomputed continuation labels, and verified 986
counterfactual/actual witness references. Their 461 unique witnesses passed
independent complete-factory native admission and original directed-topology
audits. Successful probes kept every facility pose, original endpoint/rate and
previously routed path intact. The source training run remains unchanged.

Readiness covered 11 source poses and included an order-dependent recovery.
Two native-backed tests checked retained-best reconstruction across commits and
an explicit diagnostic fork from an already-terminal source state. Workspace
check/test and the Python regression suite passed. Artifact-policy checks also
passed; generated checkpoints, cases, reports and plots stay under ignored
`artifacts/`.

The next learning question should focus on recognizing and finishing recoverable
post-pose drafts, with success sequences starting from the policy's own move.
For overlap, additional placement decisions remain necessary. For a concrete
recoverable area-improving draft, the current evidence supports examining why
repair/commit loses to another pose or restore-best. It does not yet establish
which curriculum, action abstraction or credit estimator will fix that behavior.

## Reproduction and retained evidence

The retained probe is `artifacts/factory-edit-pilot/post-pose-recovery-20260911`.
Its `report.json`, `summary.json`, `source-episodes.pt.gz` and individual cases
bind the source PPO evidence. `recovery.png` summarizes all states;
`area_then_restore.png`, `order_recovery.png` and `overlap.png` illustrate the
explicitly selected cases. The [compact provenance manifest](2026-09-11-blueprint-post-pose-recovery-provenance.json)
binds reports, source hashes, figures, selected cases and verification logs.

```sh
tools/factory-edit-pilot/.venv/bin/python tools/factory-edit-pilot/probe_post_pose_recovery.py --run artifacts/factory-edit-pilot/blueprint-ppo-20260911 --config tools/factory-edit-pilot/blueprint-post-pose-recovery.json --output artifacts/factory-edit-pilot/post-pose-reproduction
tools/factory-edit-pilot/.venv/bin/python tools/factory-edit-pilot/summarize_post_pose_recovery.py --run artifacts/factory-edit-pilot/post-pose-reproduction
```
