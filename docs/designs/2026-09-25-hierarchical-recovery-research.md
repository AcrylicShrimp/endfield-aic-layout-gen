# Hierarchical policy recovery research

Status: active. The goal is a reproducible improvement in completed layouts
and preservation of previously solved cases, not a favorable single run.
Production search and training remain at their accepted settings until an
independent confirmation passes.

The frozen horizon-12 search passed the 48-case screen and the declared
64-case confirmation. A panel-helper namespace deviation was closed
separately. The declared namespace and the helper namespace produce identical
ordered problem graphs but different case IDs; because Gumbel noise hashes
case ID, their absolute completion counts differ substantially. See the
paired [namespace result](../reports/2026-09-25-hierarchical-horizon-strict-result.md).
Learner retention and untouched held-out performance remain open.
The frozen-collection learner screen later found that successful-action
imitation alone passed its predeclared three-stream retention/completion gate;
see the [screen result](../reports/2026-09-25-hierarchical-success-imitation-result.md).
The [fresh on-policy confirmation](../reports/2026-09-26-hierarchical-success-onpolicy-result.md)
then completed all 18 training evaluations and failed four of five primary
promotion conditions. Its final completion differences were 0, -6, and +8
of 64 across the three streams. The extra seed-stress stage was skipped
because the required two winning streams were already impossible. This
target recipe is rejected as a recovery setup; the next intervention should
test search noise assignment and source-to-diagnostic transfer.

## Evidence and hypotheses

The preceding causal study found seed-sensitive root candidates, short-horizon
Q that often ranked independently successful first actions incorrectly, and a
nearly uniform full-action policy target. Neither fourfold simulation budget,
naive target sharpening, shorter updates, nor replay of the same diffuse
successful-root targets produced a confirmed retention repair. Search failures
are bounded failures; they do not establish global infeasibility.

The next tests distinguish:

1. Root selection may overuse the short-horizon Q scale. Compare the accepted
   scale `0.1` with `0.02` and `0.001`, with all other search controls fixed.
   The initial exact-zero proposal was rejected by the existing positive-scale
   configuration boundary before it could produce a search result.
2. The eight guaranteed anchor candidates may be too few or may displace
   useful model-proposed candidates. Compare 16 and zero anchors to eight.
3. The six-move horizon may truncate important consequences. Compare horizon
   12 with six at the accepted root controls.
4. The policy loss may imitate mostly untested actions, even for successful
   episodes. Compare successful-action imitation with the accepted target on
   byte-identical collections, while keeping value supervision unchanged.
5. Single-seed evaluation may exaggerate both gains and regressions. Evaluate
   promising search and learner settings across independent search seeds and
   keep paired per-case gains, losses, and compute cost.

These are initial hypotheses, not an exhaustive model of the failure. A
surprising result should trigger a narrow check of the relevant code path and
case witnesses before another expensive matrix.

An on-policy diagnostic exposed another candidate mechanism while the
three-generation confirmation was still running. For the 32 shared `s1-g1`
collection graphs, the `current` and `success-on` models produced exactly the
same first-root action sets, but only 0.23% of actions kept the same array
position on average; mean Spearman rank correlation was 0.74 and the mean
median absolute rank shift was 325 positions. The first selected action
matched in 0/32 cases. `hierarchical_search.rs` sorts edges by learned prior
before assigning deterministic Gumbel values by action-array index. Thus the
same case ID and root seed do not give the same random perturbation to a
physical action after its rank moves. This is preliminary mechanism evidence,
not proof that remapping caused the performance loss: the learned prior and
search values changed too. After closing the on-policy experiment, test a
stable action-identity Gumbel assignment on paired graphs and models, first
with a bounded root-level comparison and runtime check, then with full
multi-seed search only if the mechanism remains plausible. Preserve the
prior-sorted edge order needed for progressive widening. A stable assignment
has the same marginal independent Gumbel distribution; its proposed benefit
is reduced sensitivity to incidental action order, not a guaranteed increase
in expected search strength.
The bounded [rank-coupling probe](../reports/2026-09-25-hierarchical-onpolicy-rank-probe-result.md)
subsequently reconstructed all 64 actual candidate sets. The current
model pair shared only 8/16 visited candidates (the anchors); the
action-keyed counterfactual shared 15.81/16 initial candidates on average.
The same pattern held on 64 disjoint diagnostic cases: 8.02/16 versus
15.97/16 shared candidates, with all 128 actual candidate sets reconstructed.
The action batch is assembled before the outer model scores and sorts edges.
When the lower guidance model and state are fixed, retaining that pre-sort
ordinal on each edge may provide stable Gumbel assignment with only one
integer per edge and no per-action serialization. Verify the ordinal/action
correspondence across paired models and after placement-page widening before
using it. An action-identity hash remains the robust comparison if candidate
generation itself changes when lower training is restored. Measure runtime
and memory for either variant before a longer training run.
For an action-identity variant, seed from the explicit search seed and the
existing state SHA-256 (which includes problem and observation), then hash the
physical command witness rather than the entire candidate record. A same-ID
repeat retained the physical root action sets but changed route prediction,
work, and provenance metadata, which would otherwise remap Gumbel draws.
The source of this run-to-run variation remains to be isolated. Omit the
panel case ID: it is sampling metadata and should
not change search noise for an identical problem state. Keep rank-index
assignment as the default during the controlled comparison. This also lets
the same-graph/different-namespace panel pair test identity invariance.
The [consecutive-generation probe](../reports/2026-09-26-hierarchical-onpolicy-generation-rank-probe-result.md)
then reproduced all 512 candidate sets on the shared 64-case diagnostic.
Even `current` generation 1→2, with mean rank correlation 0.979, retained
only 8.14/16 candidates under rank-index Gumbels versus 16/16 under the
action-keyed counterfactual. This supports a candidate-churn mechanism, not
yet a measured performance repair.
The first available on-policy diagnostic report had no duplicate serialized
actions within any of its 608 root records (355,315 actions total). This is
a bounded check that action-keyed draws stay distinct on the sampled roots;
the candidate generator's general uniqueness and the new search outcomes
still need validation.
After restricting the key to physical commands, a retained 16-case search
report also had no duplicate physical keys in 154 roots (92,788 actions).

The bounded implementation check uses the first eight cases of the retained
`s1` source panel, then a byte-identical problem/prefix panel with only IDs
renamed. Run the original rank and physical-action-key variants at the same
128-simulation, horizon-six, 24-move setting, seed 20260922, lower and outer
checkpoints. Repeat each original-ID run once. Compare physical first-action
agreement, initial visited-candidate overlap, completion, wall time, and peak
resident memory across the six runs. This checks coupling and integrated
cost; eight old source cases are not a performance-promotion panel. Any
promising setting still needs disjoint paired search and fresh on-policy
confirmation.

## Search screen

Reuse the 48 generation-87 frozen training-source cases from the causal study,
regenerated with the recorded panel preparation and checked against retained
panel hashes. Keep model bytes, lower model, game data, 128 simulations, 24
maximum moves, and all unmentioned settings equal. Use root seeds 20260922,
20260923, and 20260924 for each configuration. Search arms are `base`,
`q02`, `q001`, `anchor16`, `anchor0`, and `horizon12`. `q02` and `q001` set
`value_scale` to 0.02 and 0.001. `anchor16` and `anchor0` set guaranteed
`anchor_candidates` to 16 and zero. `horizon12` sets `horizon` to 12.
Compare validated completion per 48-case seed, cases successful under at
least two seeds, failed-to-success and success-to-failed transitions against
the paired baseline, time, and first-action agreement. Do not choose by the
largest single-seed total. A search candidate merits a new panel only if it
improves the three-seed mean by at least three of 48 completions, wins at least
two seeds, and does not lose more than two baseline robust successes. Report
all arms even if none passes. A new disjoint panel is required before changing
the search contract.

If `horizon12` passes the frozen screen, compare only it and `base` on a new,
disjoint 64-case training-partition panel under the same three root seeds.
Generate 512 candidate cases with seed 20262560 and namespace
`hierarchical-recovery-horizon-confirm-v1`, excluding all 48 screen problem
identities before taking the first 64. Preserve the generation-87 checkpoints
and exact search configurations. Call horizon 12 independently confirmed only
if its mean gain is at least four of 64, at least two seeds gain, and no more
than three baseline robust successes (successful under at least two seeds)
are lost. Keep the full gain/loss masks, first-action agreement, and runtime.
This confirmation checks a frozen search setting; it is not evidence that the
learner will retain its benefits across generations.

As a mechanism check after the screen, compare each arm's first-root searched
Q and improved-policy target with the previously independently confirmed
first-action winner/loser pairs on these same cases. Count Q agreement only
when both actions were visited, and compare target pair loss on the common
set of pairs by case. This is a diagnostic reuse of previously selected
action pairs, not a fresh estimate of action quality or a promotion gate.
After the first S1 seed suggested a large horizon effect, also count roots
whose visited Q exceeds one, including the first four roots per episode. In
this search, the unfinished-state heuristic and model completion value are at
most one, so Q above one witnesses at least one completed-layout leaf. This
post-hoc mechanism count does not enter selection.
Also measure first and later-root target mass on unvisited actions and first-root
effective action fraction, because a search improvement does not establish a
better policy-learning signal.

## Learner screen and confirmation

Treat the accepted search as the control unless an independently confirmed
search setting exists. On frozen collections, rewrite only policy-target
probabilities: for fully validated episodes, put a predeclared 80% mass on the
recorded selected action and 20% on the original target; failed episodes keep
the original target in one treatment and omit policy loss through the existing
`train_failed_policy=false` setting in another. Do not change search witness,
value target, case identity or model initialization. Test an all-root variant
and a successful-first-root-only variant if the all-root result is ambiguous.
Use three independent source streams and a diagnostic panel disjoint from
their training cases. Record starting-success retention, gained cases, easy
and hard strata, and training loss separately. This screen selects at most
one treatment; positive completion alone is insufficient when retention
worsens. The first promotion gate is mean three fewer lost starting successes
than control, positive retention in two of three streams, and mean total
completion at least control minus two.

Any treatment passing a frozen screen needs three or more fresh on-policy
generations across three streams, with shared problem panels and independent
arm collections after the first generation. Use a fresh diagnostic panel,
then restore lower training and evaluate a separate untouched held-out set
before proposing a production change. Fix numeric confirmation gates before
those outcomes. If the initial treatments fail, inspect the observed failure
mode and test another concrete mechanism rather than claiming that all
policy learning is ineffective.

The `success-on` treatment passed that screen and is the sole learner arm
advanced to on-policy confirmation. Compare it to `current` for three
generations in each of three streams. Each generation uses a shared 32-case
training panel; generation 1 shares the collected report, and later
generations collect separately with each arm's own current model. Keep lower
frozen, the accepted 16-candidate/128-simulation/horizon-6 search, 100 epochs,
and the same 80% successful selected-action target. Exclude all prior 48
source problems and the 64-case frozen-screen diagnostic from the new panels.
Use a new 64-case diagnostic panel disjoint from every generation panel.

For the three-generation gate, compare both arms on that paired diagnostic
panel after generation 3. Require at least four more completed cases on
average across streams, wins in at least two streams, at least three fewer
starting successes lost on average, and positive retention in at least two
streams. Also require the candidate's mean completion advantage across
generations 2 and 3 to be nonnegative, so a final isolated rebound is not
enough. Confirm final checkpoints and starting model under search seeds
20260922, 20260923, and 20260924; the mean advantage across the nine
stream-seed pairs must remain at least four completed cases and be positive
for at least two seeds after averaging streams. Treat any failed condition as
failure to confirm. Passing this stage would still require lower-training
restoration and untouched held-out evaluation before production adoption.

## Horizon-12 on-policy transfer

The successful-action target failed that confirmation. The independently
confirmed frozen-search improvement from horizon 12 is now the stronger
candidate for a training/search setup. Reuse the exact nine 32-case training
panels and the 64-case diagnostic panel from the completed on-policy study,
regenerating them and checking their hashes against its retained protocol.
Reuse its three `current` horizon-six trajectories as the control. Start
three new horizon-12 trajectories from the same generation-87 model weights,
with lower frozen, the original policy target and 100-epoch/1600-update
training. Change only the search horizon from six to 12 and the outer model's
matching search contract. Use 128 simulations, 16 candidates, eight anchors,
rank-index Gumbels and seed 20260922. Each horizon-12 trajectory collects
its own later generations. Evaluate both setups on the same diagnostic IDs;
the new candidate is the combined horizon-12 search and the model trained
from its collections.

Compare every horizon-12 generation with the already retained horizon-six
`current` record for the same stream and generation. Use the original
horizon-six starting checkpoint's 26 successful diagnostic cases for paired
retention on both arms. The predeclared primary gate after generation three
is mean completion gain at least four of 64, at least two winning streams,
mean three fewer lost starting successes, and positive retention in at least
two streams. The mean completion advantage across generations two and three
must be nonnegative. If all primary conditions pass, evaluate final models
and the starting checkpoint under root seeds 20260922/23/24; require mean
final gain at least four across the nine stream-seed pairs and positive
stream-averaged gain for at least two seeds. A failed primary condition
rejects promotion without the extra seed stage. A promising result still
needs a same-search cross-evaluation to separate model transfer from search
strength, then lower-training restoration and untouched held-out evaluation.

If the seed-stress gate passes, complete the two missing cells of a paired
two-by-two comparison for each stream on the same 64-case diagnostic and root
seed 20260922: evaluate the horizon-six-trained final model with horizon-12
search, and the horizon-12-trained final model with horizon-six search. Reuse
the two diagonal cells already measured in the on-policy experiment. Change
only the outer model's search contract when creating a shadow checkpoint;
verify its weight hash matches the original. Report completion and loss of
the 26 starting successes for all four cells, model differences within each
search horizon, and search differences within each model origin. This stage
explains which factor carries the measured gain; it does not replace the
predeclared seed-stress gate or the later untouched held-out test.

The completed cross-evaluation selected a narrower follow-up: keep the
horizon-six `current` final model weights but use horizon-12 search. The
seed-20260922 cross cells are selection evidence, so confirm this mixed setup
on the other two prespecified root seeds, 20260923 and 20260924, without
changing the diagnostic panel, lower weights, search controls or case IDs.
For each of three streams and both seeds, compare the new mixed report with
the retained H6-model/H6-search and H12-model/H12-search reports. Use the
H6-search starting-model success mask for that seed as the common retention
baseline. Before seeing mixed outcomes on these seeds, require the mixed
setup versus H6/H6 to gain at least four completions on average across six
pairs, to win both seed-averaged completion comparisons, and to lose at least
three fewer starting successes on average. To prefer mixed over H12/H12,
also require its mean completion difference to be at least minus two cases,
each seed mean to be at least minus two, and its mean number of starting
successes lost to be no greater. Report every paired gained and lost case;
these thresholds do not turn the reused diagnostic into an untouched test.

## Continuity and closeout

Reuse the existing CLI search harness, panel generator, generation trainer,
batching, checkpoint format, and nonblocking W&B observer. Run CUDA work on
the dedicated server under `~/Srv` after disk/GPU checks. Verify W&B readiness
before leaving a long training run unattended. Keep compact case-level
evidence, exact code/config/checkpoint identities, runtime, reproduction
commands, interpretation, and a cleanup manifest for each completed stage.
Remove replayable raw reports only after their compact evidence is readable.
