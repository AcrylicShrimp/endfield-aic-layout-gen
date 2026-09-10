# PPO action contributions to best-area reduction

The fresh routing/reward experiment reduces area earlier in each episode and
gets roughly the same area contribution from substantially fewer pose attempts.
Its later actions contribute less than the previous policy's, however, and most
of the increased rerouting leaves physical state unchanged. Lower failure rates
are a real observed change, but do not by themselves establish better exploration.

## Scope and measurement contract

The audit covers cumulative batches 0 through 512, every eight batches: 65
evaluations per run, 32 identical initial factories per evaluation, 4,160 complete
episodes and 133,120 recorded actions across both runs. Every trace is present.
The old curve combines original CNN-2 batches 0–32 with the stopped continuation,
offset by 32. The recent comparison is cumulative batches 456–512: eight
evaluations, 256 episodes and 8,192 actions per run.

Direct contribution is `(best_area_before - best_area_after) / initial_area`.
Contributions are assigned to the action that realizes the best-area decrease,
then averaged equally across episodes. They sum to the reported mean area
reduction; they do not independently measure the causal value of earlier actions.
Failure penalties are excluded. Raw tile decreases are normalized by each
factory's own initial area so large factories do not dominate the result.

The diagnostic accepts explicit job directories, cumulative batch offsets, an
inclusive cutoff and a fresh output path. It emits per-kind/per-position counts,
area contributions, observational follow-ups and a hashed input manifest. It
rejects missing traces, duplicate evaluation batches, different evaluation input
cohorts, discontinuous best areas, inconsistent rewards/counts, incorrect physical
unchanged flags, and disagreement with the phase's recorded area quality.

The same initialization and first 32 training batches were controlled. Later
training starts can differ because generation uses the modified router. These
are one-seed observations on 32 fixed evaluation factories, not an isolated
penalty ablation or evidence of generalization to new factories.

## Recent direct contributions

| Action | New selection share | New area-improvement rate within kind | Old contribution | New contribution |
| --- | ---: | ---: | ---: | ---: |
| `contract` | 10.85% | 87.51% | 53.4850 pp | 52.9608 pp |
| `pose` | 32.48% | 18.60% | 10.5132 pp | 10.6041 pp |
| `restore_best` | 20.84% | 0% | 0 pp | 0 pp |
| `port` | 15.80% | 0% | 0 pp | 0 pp |
| `reroute` | 16.66% | 0% | 0 pp | 0 pp |
| `insert` | 2.91% | 0% | 0 pp | 0 pp |
| `component` | 0.27% | 0% | 0 pp | 0 pp |
| `release_bridge` | 0.20% | 0% | 0 pp | 0 pp |
| Total | 100% | — | 63.9982 pp | 63.5649 pp |

Contract and pose account for 83.32% and 16.68% of the new policy's direct area
reduction. The new policy attempts pose 2,661 times versus 4,350 previously;
pose failure drops from 60.53% to 27.36%, while direct area improvement per pose
selection rises from 11.72% to 18.60%. Pose therefore preserves its aggregate area
contribution with 38.83% fewer attempts. This combines policy, router and visited
state changes; it cannot be attributed to any one change alone.

Correction to the earlier conversational summary: `status=improved` includes
lower-priority transport-tile/turn improvements. Its old/new frequencies of
18.13%/18.21% were incorrectly labeled area-improvement frequencies. Strictly
positive area reductions occur on 16.44%/15.54% of all actions. For example,
107 new port actions improve the score, but none reduces best area in this window.

## Contribution over learning and within an episode

| Training window | New contract | New pose | New actions 9–32 | Old actions 9–32 |
| --- | ---: | ---: | ---: | ---: |
| 8–64 | 48.10 pp | 8.62 pp | 6.83 pp | 5.32 pp |
| 72–128 | 51.62 pp | 10.34 pp | 3.73 pp | 3.60 pp |
| 200–256 | 53.16 pp | 9.80 pp | 2.76 pp | 2.71 pp |
| 264–320 | 52.85 pp | 10.37 pp | 1.98 pp | 2.99 pp |
| 456–512 | 52.96 pp | 10.60 pp | 1.18 pp | 3.48 pp |

In the recent window, the first eight actions contribute 62.3844 pp in the new
run versus 60.5150 pp previously. This is 98.14% versus 94.56% of final area
reduction. Later action blocks contribute:

| Episode actions | Previous | New |
| --- | ---: | ---: |
| 9–16 | 2.8545 pp | 0.9456 pp |
| 17–24 | 0.4867 pp | 0.1380 pp |
| 25–32 | 0.1419 pp | 0.0969 pp |

The new policy's early advantage is 1.8694 pp; its later contribution is 2.3028 pp
smaller, leaving final area reduction 0.4333 pp below the old policy. Earlier
improvements naturally leave less progress available later, so this is a useful
localization of the difference, not proof that every late action is wasteful.

## Preparation, unchanged states and observational follow-ups

New rerouting leaves physical state unchanged in 1,361/1,365 selections (99.71%).
All 352 old reroutes do so. This is direct evidence that the additional reroutes
usually do not prepare a different physical layout. Action history and subsequent
policy inputs can still change even when physical state does not.

New port actions change physical state in all 1,249 accepted selections and can
improve lower-priority scores, so their zero direct area contribution does not
establish uselessness. Among non-area-improving accepted actions with a complete
four-action future window, an area gain occurs before the next `restore_best` in:

- Port: 50/1,025 (4.88%) new, 35/374 (9.36%) old.
- Pose: 437/1,307 (33.44%) new, 521/1,060 (49.15%) old.
- Reroute: 10/1,100 (0.91%) new, 0/278 old. All ten new associations follow an
  unchanged reroute; none of the four changed reroutes has such a follow-up gain.

These windows overlap and occur in different states/episode positions. Their
gains must not be added as action contributions, and the percentages are not
causal preparation success rates. A later skip/replay experiment would need to
control successor actions and account for altered action availability/history.

The immediate research target is the later part of the episode: distinguish
useful temporary relocation from physical repeats, and determine whether a port
change enables a specific later improvement. This audit changes no action masks,
reward coefficients, model parameters or running training configuration.

## Failure-cost pressure late in an episode

The user's hypothesis is that penalizing failure encourages neutral repeats.
The reward contract supports this mechanism: failure costs 0.001, while a
physically unchanged accepted action contributes zero. The coefficient is small
relative to the whole episode's area gain, but not relative to its late gains.
In the recent new-policy evaluation window:

| Episode actions | Direct area gain | Failure cost | Gain minus cost |
| --- | ---: | ---: | ---: |
| 1–8 | 62.3844 pp | 0.0359 pp | 62.3485 pp |
| 9–16 | 0.9456 pp | 0.0984 pp | 0.8471 pp |
| 17–24 | 0.1380 pp | 0.0941 pp | 0.0439 pp |
| 25–32 | 0.0969 pp | 0.0809 pp | 0.0160 pp |

These are mean sums of recorded immediate reward components in each block, not
PPO advantages or controlled action values. In the final eight actions, the
failure cost is 83.48% of direct area gain. Neutral repetition can therefore be
attractive where the policy expects little further improvement. The unchanged
reroute frequency fits this explanation, but the joint router/reward change and
different visited states prevent assigning causality to the penalty alone.

A useful next controlled comparison would keep the improved router fixed and
vary only the failure coefficient under matched initialization/input schedules,
measuring late area gains and physical repeats as well as failure rate. This
report does not launch that comparison or alter the ongoing twelve-hour run.

## Reproduction and verification

From the isolated diagnostic host directory, with `srv=/home/ashrimp/Srv`:

```sh
python audit_ppo_action_contributions.py \
  --run "new=0=$srv/aic-ppo-cnn2-routing-12h-20260910/artifacts/factory-edit-pilot/cnn2-routing-twelve-hour/current-history-16--seed-17" \
  --run "old=0=$srv/aic-ppo-spatial-20260910/artifacts/factory-edit-pilot/ppo-spatial-cnn2/current-history-16--seed-17" \
  --run "old=32=$srv/aic-ppo-cnn2-12h-20260910/artifacts/factory-edit-pilot/cnn2-twelve-hour/current-history-16--seed-17" \
  --through 512 --output artifacts/action-contributions-512-reproduction
python plot_ppo_action_contributions.py \
  --report artifacts/action-contributions-512-reproduction/report.json \
  --output artifacts/action-contributions-512-reproduction/contributions.png
```

The plot requires matplotlib; the existing server training environment provides
it. The audit itself uses only the Python standard library. Output identities and
locations are recorded in the adjacent provenance manifest. Generated outputs
remain in ignored artifacts; the original traces are retained on the server.

Verification passed: the full CLI audit and its conservation/cohort assertions;
three focused tests covering secondary-score attribution, restore boundaries,
separate failure costs and corrupt reward/physical flags; manual plot generation and visual inspection;
`cargo check --workspace`; `cargo test --workspace`; tracked-file checks before
and after staging. No Rust source or production training imports changed.
