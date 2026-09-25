# Hierarchical search-target causal study

Status: staged research protocol. No treatment is a production setting until the
learner confirmation and retention checks pass.

## Question and retained baseline

The generation-87 frozen-model audit found about 2,734 first-root macros per
case, 16 visited macros, a nearly uniform completed-Q policy target, and little
change in target spread or completion after increasing the same 16-candidate
search from 128 to 512 simulations. The audit does not establish that a better
action was excluded, that sampled Q-values predict continuations, or that the
policy target causes later case churn. Resolve these three uncertainties in
that order. Reuse its three disjoint 16-case training-source panels and exact
lower and outer checkpoint hashes. Do not use historical validation or final
held-out cases for treatment selection.

## 1. Candidate coverage at fixed search cost

Regenerate the 48 frozen cases with the existing panel preparation and verify
their SHA-256 hashes against the retained audit protocol before search. Use
the existing eager search harness, model weights, root seed, root prior mix,
eight guaranteed anchor actions, and all other controls unchanged. Compare
candidate counts 16, 32, and 64 at both 128 and 512 simulations per move;
include 128 candidates at 512 simulations as a coverage stress cell. Generate
the matching outer model configuration for each search configuration while
keeping checkpoint bytes identical. Retain one baseline 16/128 cell in this
new run so exact action witnesses can be compared with every other cell.

For every paired first root verify state and complete action-witness identity.
Record considered-set overlap, chosen-action membership in the 16-action set,
search Q and visits, completed-Q target entropy, target-prior divergence,
unvisited mass, model and anchor prior entropy, prior mass on visited actions,
full episode completion, conditional quality, and integrated wall time. Compare
cases gained and lost, not only aggregate completion.
Wider search may trade depth for coverage; neither a count gain nor a flat
result alone settles action quality. Inspect cases with unexpected nonnested
candidate sets, near-tied search rankings, or inconsistent first-root states
before interpreting the matrix.

## 2. Independent first-action continuation

For each case where the 16/128 and a wider arm select distinct first actions,
compare both actions by applying their exact recorded macro witnesses to the
same frozen prefix, then continuing with the same subsequent search contract.
Include the highest- and lowest-Q actions among the original 16 to test value
ranking, and the highest-Q action among the eight guaranteed anchor actions.
Outside the original 16, include the highest-anchor action, the
highest-mixed-prior action, the highest-Q visited action in the
128-candidate/512-simulation cell, and one deterministically hashed action as
a base-rate control. Deduplicate exact witnesses. Keep the original case ID;
use downstream root seeds 20260922, 20260923, and 20260924 and 23 remaining
macro moves for every forced first action. Run variants as panel batches to
reuse model loading and compiled inference. Verify every recorded witness is
accepted by native prefix replay and its resulting state hash agrees across
seeds. The baseline-selected witness must reproduce the original second-root
hash when that root exists. First-root action selection is not rerun in these
continuations.

Report validated completion frequency, partial connection progress, and
conditional layout quality per paired action and seed. Separate wider actions
that were outside the original 16 from changed choices inside it. This is a
bounded continuation comparison, not an optimal-action oracle: later search
can still fail and action-dependent state hashes change later Gumbel draws.
Use repeated seeds and paired case summaries to expose that uncertainty. Call
one action a *clear continuation winner* over another only if its completion
count exceeds the other's by at least two of the three seeds; equal or
one-seed differences remain unresolved. Report all outcomes regardless of this
label.

If fewer than 20 clear pairs remain or a claimed outside-16 advantage changes
with downstream seed, rerun only those unresolved first-action pairs with 512
simulations per later move before judging coverage or Q ranking. Keep that
escalation separate from the primary 128-simulation outcomes.

## 3. Target construction and learner effect

On the same frozen root records, compare the present full completed-Q target
with prespecified, search-independent target transforms: Q-logit amplification
of 2x and 4x relative to the stored mixed prior, and a diagnostic target
that puts a fixed half of its mass on the conditional visited-action target
while retaining half on the mixed prior. These are diagnostic alternatives,
not assumed improvements. Reconstruct and check normalization and witness
identity exactly. Measure each target's mass and rank on the forced-action
continuation winners, entropy, prior divergence, mass on unvisited actions,
and agreement across search budgets and seeds. Measure these at first roots
and later roots separately; later roots have much smaller action sets.

Use a pairwise logistic score on the clear continuation winners, conditioned
on the tested action set, to select at most one alternative: require at least
20 clear action pairs, an overall loss improvement of at least 0.05 nats over
the current target, and improvement in at least two of the three panel
streams. Break any tie in favor of 2x, then 4x, then half-visited. If none
passes, use the half-visited target only as a prespecified stress contrast for
the learner; label outcome alignment unresolved and do not promote it. First
train paired current-target and contrast-target arms from identical starting
weights on byte-identical frozen search collections to isolate the target
change. Materialize each research contrast by rewriting only the
`policy_target` fields, with the source and transformed report hashes recorded
outside the search report. Compare first-root-only and all-root transforms in
this one-generation probe before choosing the scope for continued training.
Then compare at least three streams of multi-generation retention using fresh
on-policy collections for each arm and the same problem streams. Keep search,
lower model, optimizer updates, replay, and evaluation cases equal. Compare
immediate gains/losses and retention,
including easy and hard strata. A lower training restart and untouched final
held-out test are required before adopting a learner change for the
curriculum.

## Controls and closeout

- Record source commit, executable and checkpoint hashes, panel/configuration
  hashes, commands, actual runtime, and any deviations before interpreting
  results. Keep raw reports only until compact case-level evidence is verified.
- Run CUDA work on `ashrimp-arch` under a task-specific `~/Srv` checkout after
  disk and GPU checks. Before an unattended long learner run, verify W&B has
  acknowledged a readiness record through the existing nonblocking observer.
- Close and clean each stage before launching the next substantial stage.
  Preserve exact configurations and compact evidence under `docs/`; remove
  regenerable server artifacts only after the closeout is readable and record
  byte-precise cleanup.
- Report observed associations and causal limits separately. In particular,
  a wider candidate set can improve search without making the policy target
  correct, and a sharper target can fit a chosen continuation while harming
  later retention.
