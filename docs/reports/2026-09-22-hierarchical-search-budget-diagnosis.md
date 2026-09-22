# Hierarchical search budget diagnosis

## Question and verdict

This diagnosis tested whether additional MCTS simulations alone can recover the
fixed 32-case curriculum-regression panel efficiently. It followed the
[`diagnostic contract`](../designs/2026-09-22-hierarchical-search-budget-diagnosis.md)
and changed only `simulations_per_move` while retaining the generation-nine
lower and outer weights, search formulation, action generator and panel.

**Verdict: reject budget scaling as the production treatment.** No tested
budget reached the 27/32 qualification threshold. Completion improved from
9/32 at 64 simulations to 19/32 at 1,024 simulations, then remained 19/32 at
2,048. The unchanged solver's diagnostic knee is 1,024 simulations, but using
it for curriculum collection would multiply search work by 16 while remaining
11 cases behind the 30/32 anchor-prior control.

The result supports a separate Gumbel/anchor allocation experiment. At 1,024
simulations all 21 known recovery first actions are eligible under progressive
widening, yet the root visits only 9.25 actions on average. More simulations
open candidates; they do not guarantee that MCTS evaluates them.

## Controlled matrix

The release binary SHA-256 was
`065f2187dae0cc951daa8d7fb315e55aef6e9c080a510fd2afd5784d86f754e7`.
The fixed panel SHA-256 was
`976bceeb93bf935578e862ce6d715184f6f3be939c3d736854ba74a56071d873`.
Every cell used lower checkpoint
`bc68bbb1e819d2958e9388f4e07fc35dab059a14056c936dfce7743ef013ad77`
and outer checkpoint
`5e11cf22c7924581aeb43de21361953d38e0022bf8d085bfda8ada827ea6a030`.
Cloned outer directories changed only their recorded search contract; every
cloned state hash matched the retained generation-nine weights.

The compact machine-readable results, including exact configuration and raw
report hashes, are in the
[`budget evidence`](2026-09-22-hierarchical-search-budget-evidence.json).

| Simulations per move | Completed | Easy control | Early hard success | Early hard failure | Wall seconds | Known recovery root actions eligible |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 64 | 9/32 | 6/8 | 3/17 | 0/7 | 13.24 | 0/21 |
| 128 | 9/32 | 8/8 | 1/17 | 0/7 | 21.58 | 5/21 |
| 256 | 9/32 | 8/8 | 1/17 | 0/7 | 30.73 | 7/21 |
| 512 | 12/32 | 8/8 | 4/17 | 0/7 | 42.67 | 20/21 |
| 1,024 | 19/32 | 8/8 | 11/17 | 0/7 | 67.56 | 21/21 |
| 2,048 | 19/32 | 8/8 | 11/17 | 0/7 | 91.13 | 21/21 |

The 128-simulation cell exchanged three successes for three others rather than
retaining the 64-simulation set. The 256-simulation cell matched that exact set.
The 512 and 1,024 cells then added three and seven successes monotonically.
Doubling from 1,024 to 2,048 changed no case outcome.

## Eligibility is not evaluation

For the current rule, an action at prior rank `r` first becomes eligible after
`(r - 1)^2 + 1` parent visits. The 21 actions used by the anchor control to
recover learned-policy failures have learned-prior ranks from 12 through 29.
This predicts zero root coverage at 64 simulations, 20/21 at 512 and 21/21 at
1,024. The observed completion transition appears in the same interval, so the
rank calculation is useful as an impossibility filter.

Eligibility alone overstates useful exploration. Initial-root behavior was:

| Budget | Eligible prefix cap | Mean visited root actions | Selected action is prior top | Mean selected visit share |
| ---: | ---: | ---: | ---: | ---: |
| 64 | 8 | 2.59 | 90.6% | 85.2% |
| 128 | 12 | 4.06 | 53.1% | 72.9% |
| 256 | 16 | 4.84 | 87.5% | 70.8% |
| 512 | 23 | 6.91 | 56.3% | 54.1% |
| 1,024 | 32 | 9.25 | 25.0% | 52.6% |
| 2,048 | 46 | 10.88 | 43.8% | 66.7% |

At the knee, more than two thirds of the eligible prefix remains unvisited on
average. At 2,048 simulations the eligible prefix grows by 14 actions, but the
mean number visited grows by only 1.63 and completion does not change. Standard
PUCT allocation therefore continues to concentrate visits after progressive
widening admits additional actions.

The fixed seven early hard failures are the clearest separation. The anchor
control solves six of them, while no learned-prior budget through 2,048 solves
any. This rules out insufficient root simulations as the complete explanation.
Candidate allocation and deeper-node exploration remain causal blockers.

## Cost frontier and decision

The Pareto frontier contains 64, 512 and 1,024 simulations. The 128- and
256-simulation cells cost more without improving completion over 64, and 2,048
costs more without improving completion over 1,024.

Moving from 256 to 512 costs 11.94 additional wall seconds and recovers three
cases, or 3.98 seconds per added completion on this panel. Moving from 512 to
1,024 costs 24.88 seconds and recovers seven cases, or 3.55 seconds per added
completion. The next doubling costs 23.57 additional seconds and recovers none.

Use 1,024 only as the comparison point for the next bounded exploration
experiment. Retain 64 as the unchanged production baseline and do not resume
curriculum training with a larger simulation count. A Gumbel/anchor allocator
should be compared at 64, 128 and, only if still informative, 256 simulations;
its purpose is to obtain the coverage currently requiring 1,024 simulations
without paying that budget at every move.

That comparison is complete. The
[`adapted Gumbel root search`](2026-09-22-adapted-gumbel-root-search.md)
combines guaranteed anchor candidates with a conservative root proposal prior
and completes 27/32 cases at 128 simulations in 39.71 seconds. It replaces the
64-simulation PUCT curriculum setting.

## Retention state

The server retains exact commands, six search configurations, compact evidence
under `artifacts/hierarchical-search-budget-20260922`. All six raw report hashes
were verified before the reports and cloned checkpoint directories were removed,
reducing the retained diagnosis from 112 MiB to 192 KiB. No training or held-out
evaluation occurred.
