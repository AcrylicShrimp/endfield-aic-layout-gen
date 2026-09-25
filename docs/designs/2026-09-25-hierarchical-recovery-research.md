# Hierarchical policy recovery research

Status: active. The goal is a reproducible improvement in completed layouts
and preservation of previously solved cases, not a favorable single run.
Production search and training remain at their accepted settings until an
independent confirmation passes.

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

## Continuity and closeout

Reuse the existing CLI search harness, panel generator, generation trainer,
batching, checkpoint format, and nonblocking W&B observer. Run CUDA work on
the dedicated server under `~/Srv` after disk/GPU checks. Verify W&B readiness
before leaving a long training run unattended. Keep compact case-level
evidence, exact code/config/checkpoint identities, runtime, reproduction
commands, interpretation, and a cleanup manifest for each completed stage.
Remove replayable raw reports only after their compact evidence is readable.
