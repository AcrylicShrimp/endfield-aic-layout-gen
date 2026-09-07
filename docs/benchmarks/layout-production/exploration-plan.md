# Degrees of freedom and learning-method exploration

Status: first experiment design, 2026-09-07. Product integration is deferred until capability
and learning evidence warrant it. The final Heavy Xiranite and visual quality gates remain.

## First separate capability from learning

The old policy chooses single/connected-pair cardinal translations by one or two cells and
whole-grid empty-strip compaction. The repair kernel also supports coherent rotations, port
changes, larger groups and structural rerouting, but the old RL semantic key explicitly freezes
rotations/ports. Adding action tokens without changing that contract would reject valid new
actions as semantic violations. A new experimental environment must preserve the original
production request and independent validation, not obsolete frozen physical assignments.

First run finite, charged action-neighborhood probes on a frozen set of complete **development**
factories, including small validation cases and both loose and strong Heavy Xiranite incumbents.
Every candidate starts from the same root within a one-step probe; no successful candidate is
silently used as the next root. Follow up promising or blocked pairs with bounded two-step
exploration, because zero one-step improvement is not evidence that an action is useless.
Do not use or tune on the final sealed arbitrary-factory test suite.

## Action axes

| Axis | Controlled levels | Question |
| --- | --- | --- |
| Translation distance | 1–2, 1–4, 1–8 cells | Do longer moves reach new packing opportunities or mainly cause collisions? |
| Facility rotation | none; all allowed catalog rotations; rotation plus a bounded shift | Is orientation itself useful, or does it need simultaneous relocation? |
| Port assignment | fixed; one compatible vacant port; endpoint-pair choices | Are fixed port choices preventing routes or compact adjacency? |
| Coherent group size | singleton/pair; shared-terminal closure; groups up to 4/8/16 | Does necessary coupled motion outweigh combinatorial growth? |
| Route repair | incident/obstructed paths; standalone reroute; changed route order | Is geometry viable but the fixed router choice failing to embed it? |
| Structural interface | retained junctions; bounded junction movement/bridge release | Which frozen anchors cause measured failures? |

These are operator bounds for controlled experiments, not game constraints. Start with
single-axis changes, then rotation+shift, move+port, and group+interface interactions.
Temporary footprint expansion and bounded destroy/rebuild are later experiments if the
current-extent restriction is the demonstrated obstruction; never silently introduce them.
Use pipe-first repair throughout.

For each root/axis/level report raw domain size, enumerated fraction, screened candidates,
screening time, actual routing attempts/expansions, validation calls, unique valid witnesses,
strict improvements, best exact score, XY voids, candidate generation cost and full elapsed
time. Use matched attempt/expansion/time caps for fair controls and also report domain coverage.
An exhaustive result is exhaustive only if the complete declared finite domain was enumerated
without time/route unknowns; a shortlist cannot prove no opportunity exists.

## Negative-result taxonomy

- `static-rejected`: bodies, retained terminals, shared owners or bounds make this concrete
  action illegal in the declared neighborhood; record the exact entity and rule.
- `repair-failed`: static geometry passes but chosen routing fails; separate no path under
  fixed choices from expansion-budget unknown and untried route orders.
- `valid-no-gain`: valid actions exist, but no sampled one-step score improves. Check whether
  a short detour or a coupled degree of freedom changes the conclusion.
- `opportunity-not-learned`: a charged oracle probe finds useful actions but the learned
  policy misses them under matched inference budget. Investigate observation/credit/selection.
- `cost-dominated`: extra freedom produces useful outcomes but loses at equal elapsed time;
  compare factorized selection, shared indexes and reduced candidate duplication.
- `no-evidence-yet`: domain coverage or repetitions are insufficient. Do not turn this into
  a claim that the freedom is intrinsically unhelpful.

## Training and representation comparisons after viable actions exist

1. Keep current terminal-comparison policy gradient as a measured control, with matched
   per-root training action exposure and unchanged lexicographic incumbent acceptance.
2. Compare imitation from independently validated, charged search trajectories followed by
   RL against RL from scratch. Expert generation, negative examples and provenance are part
   of the training budget; do not give an imitation learner free oracle data.
3. Compare temporal credit with a learned value/baseline and short rollout updates against
   the current whole-trajectory constant signal. Define any scalar training surrogate and its
   limitations explicitly; do not claim the changing peer-rank reward has a shaping theorem.
4. Compare a flat action-conditioned scorer with region/group -> operator -> parameters
   selection when the action cross-product grows. Match available actions, not just parameter
   count; factorization can reduce compute but also make coupled choices harder.
5. Test compact route-occupancy/port observations and feasibility auxiliaries against the
   current lossy network-box summaries. Candidate outcomes remain training labels, never
   leaked inference features. Charge preflight and repair work separately.
6. Include already improved states and late-trajectory states with small-root rehearsal.
   Treat curriculum, horizon, architecture and action freedom as separate experimental axes.

The [NeuRewriter paper](https://proceedings.neurips.cc/paper/2019/hash/131f383b434fdf48079bff1e44e2d9a5-Abstract.html)
provides a primary example of region/rule factorization and actor-critic local rewriting.
[Wu et al.](https://arxiv.org/abs/1912.05784) studies iterative learned routing improvement.
They motivate alternatives, not expected gains on this factory problem. The local action
and validation evidence determines which comparison is worth implementing first.
