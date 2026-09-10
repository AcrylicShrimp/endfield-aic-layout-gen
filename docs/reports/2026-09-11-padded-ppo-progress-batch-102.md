# Padded PPO action analysis through batch 102

The active run had committed **102 batches / 205,376 training actions** about
29 minutes after its 01:10:31 KST start. Training and W&B were running without a
diagnostic error. This analysis freezes that prefix while learning continues;
it neither changes the runtime nor stops the twelve-hour session.

The fixed-cohort mean area reduction improved from **47.12% at initialization to
63.12% at batch 100**. Most improvement occurred before batch 44; evaluations
44-100 mostly fluctuate around 62-63%. The policy has learned faster initial
compression, while improvement after the first eight actions remains small.

## Scope and measurement

- Recent evaluation: batches 84, 88, 92, 96 and 100; the same 32 factories repeated
  five times, 160 episodes and 5,120 actions. These are not 160 independent factories.
- Latest evaluation: batch 100, 32 factories and 1,024 actions.
- Recent training: batches 83-102, 1,257 admitted episodes and 40,224 actions.
- Early training comparison: batches 1-20, 1,262 episodes and 40,384 actions.

Complete evaluation traces verify per-action best-area reduction, physical-state
identity, failure cost, action/status counts and telescoping total contribution.
Contribution means the decrease in episode-best area divided by that episode's
initial area, before failure costs. It is realized attribution, not a causal
ablation. A score improvement can improve transport or turns without reducing
area: recent evaluation has 898 `improved` statuses but only 799 area improvements.

Training summaries use all committed result rows, including phases without full
traces. Per-kind training rejection rates are therefore not inferred; the
per-kind rejection table below uses complete evaluation traces. Failure counts
use action-weighted denominators; area contributions average equally over episodes.

## Execution rejections

| Scope | Rejected / selected | Rejection rate | Routing-budget rate |
| --- | ---: | ---: | ---: |
| Initial evaluation | 142 / 1,024 | 13.87% | 0.78% |
| Recent five evaluations | 509 / 5,120 | 9.94% | 0.90% |
| Latest evaluation, batch 100 | 78 / 1,024 | 7.62% | 0.88% |
| Recent training, batches 83-102 | 4,912 / 40,224 | 12.21% | 0.93% |

Rejection is not monotonically decreasing: evaluation reached 19.24% at batch 64
before falling. Training rejection was 11.64% in batches 1-20, so it has not
declined across these two training windows. Changing factory inputs and inherited
starts make raw training quality/rates less controlled than fixed evaluation.

Of 509 recent evaluation rejections, 439 (86.25%) report `local-no-path`, 46
(9.04%) report `local-work-budget`, 18 port assignment, four endpoint/body
conflicts and two move-bound failures. `pose` accounts for 460 (90.37%) of all
rejections. No first-attempt routing-budget failures were recorded.

The separate region tier reports 336 attempt-exhausted, 128 fixed-boundary and
45 work-exhausted failures. Those describe the same rejected actions at another
level; do not add them to the local counts. Bounded local routing failure is
not proof that a valid whole-factory solution does not exist.

## Contributions by action

Recent five evaluations:

| Action | Selections | Selection share | Rejection rate | Direct area contribution, pp | Share of area gain |
| --- | ---: | ---: | ---: | ---: | ---: |
| `contract` | 677 | 13.22% | 0% | 52.38 | 83.59% |
| `pose` | 1,618 | 31.60% | 28.43% | 10.29 | 16.41% |
| `restore_best` | 1,675 | 32.71% | 0% | 0 | 0% |
| `port` | 592 | 11.56% | 5.57% | 0 | 0% |
| `insert` | 326 | 6.37% | 0% | 0 | 0% |
| `reroute` | 210 | 4.10% | 0% | 0 | 0% |
| `component` | 16 | 0.31% | 100% | 0 | 0% |
| `release_bridge` | 6 | 0.12% | 0% | 0 | 0% |

`contract` is the dominant direct contributor: 476/677 selections (70.31%)
reduce area. `pose` has both substantial failures and useful gains: 323/1,618
(19.96%) reduce area. Removing it based only on its rejection rate would ignore
its measured contribution. The sixteen `component` selections are a small sample.

Zero direct area contribution does not establish that an action is useless.
For example, 61 `port` selections improve a secondary score, and changed
placements/ports may enable subsequent improvements. The next-four-action
associations in the raw audit overlap and must not be treated as causal credit.

However, physical repetition is measurable. All 210 `reroute` selections leave
the physical state unchanged. Of 1,675 `restore_best` selections, 739 (44.12%)
also leave it unchanged, while 936 restore a different physical state. Accepted
physically unchanged actions total 949/5,120 (18.54%). This excludes the 509
rejected actions, which also leave physical state unchanged. History can still
change, so a physical repeat is not necessarily an identical policy observation.

## Early improvement and inherited starts

| Action positions | Mean area contribution, pp | Rejection rate |
| --- | ---: | ---: |
| 1-8 | 59.24 | 4.53% |
| 9-16 | 2.61 | 9.14% |
| 17-24 | 0.57 | 11.80% |
| 25-32 | 0.26 | 14.30% |

The first eight actions deliver **94.52%** of the recent total 62.67 pp gain.
The remaining 24 deliver 3.44 pp. At initialization those later actions delivered
15.88 pp: progress has moved earlier in the episode, while the final layout also
improves. Declining later contribution alone does not mean learning regressed.

Failure costs average 0.318 pp per episode, versus 62.67 pp of area gain. In the
last eight actions, however, costs consume 0.114 pp against 0.255 pp gain. The
relative effect is larger late in the episode, but this run cannot isolate the
penalty's causal effect from policy, candidate and state changes.

Recent training confirms limited extra improvement on already optimized starts:

| Start depth | Episodes | Additional area reduction | Area-improving actions | Rejection rate |
| ---: | ---: | ---: | ---: | ---: |
| Fresh | 1,001 | 61.18% | 15.00% | 11.05% |
| One completed pass | 128 | 1.01% | 0.83% | 17.19% |
| Two completed passes | 128 | 0.38% | 0.27% | 16.36% |

Inherited reductions use each pass's starting area, not the original factory's
area. Harder already-improved starts are expected to offer smaller gains; these
numbers alone do not establish a curriculum bug or a ceiling on possible layouts.

## Why batch progress is now faster

| Training window | Mean PPO epochs | KL stops | Median update | Median complete training phase |
| --- | ---: | ---: | ---: | ---: |
| Batches 1-20 | 3.70 | 4 / 20 | 15.24 s | 23.39 s |
| Batches 83-102 | 1.35 | 20 / 20 | 4.10 s | 13.11 s |

The configured maximum remains four epochs. Recent mean final approximate KL is
0.02749 against the 0.02 stopping threshold; training entropy averages 1.84,
versus 3.93 early. The faster learner implementation helps, but fewer optimizer
passes also explain accelerating batch counts. This does not mean a batch now
performs the same optimization work faster, nor does it establish instability.

The useful next observations are sustained fixed-cohort improvement beyond the
current plateau, contribution after action eight, physical repeats, inherited
start gains and KL-limited update counts. No training setting was changed.

## Reproduction and verification

```sh
python tools/factory-edit-pilot/profile_ppo_progress.py --run "$RUN" --through 102 --training-window 20 --evaluation-window 5 --output "$FRESH_OUTPUT" --plot
```

The script uses CPU-only record/trace reading. Matplotlib is optional for the
figure and was already available in the server diagnostic environment. It freezes
the observed state file, verifies durable record hashes and cohort identity,
and writes the complete input hash manifest. Two focused tests check weighting,
overlapping failure-code tiers and inconsistent counters. Workspace check/tests,
the manual diagnostic CLI and artifact-policy checks passed. The generated
figure was visually checked; raw reports, traces, state snapshots and plots stay
under ignored artifacts.

Runtime and retained evidence identities are in the
[compact provenance](2026-09-11-padded-ppo-progress-batch-102-provenance.json).
