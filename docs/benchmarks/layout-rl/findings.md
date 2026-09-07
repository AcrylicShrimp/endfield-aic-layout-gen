# Multi-step CPU layout RL: measured findings

Status: complete, 2026-09-07. Mechanism test passes; a generally superior learned improver is
**not established**. Keep this as an experimental environment, not a production cutover.

Terminal credit learns a necessary local detour that immediate credit suppresses. On held-out
roots, however, it does not consistently beat uniform nonmonotone exploration. It can beat
the tested greedy/best-first controls on small factories at one second, but untrained and
uniform controls show similar gains. The older large factories favor geometric search.
CPU parallelism helps materially; lack of available CPU is not the demonstrated quality blocker.

## Scope

The environment uses real **complete** generated factories, not disconnected toy placements
or the earlier one-step experiment's internal fragments. Fresh Rust geometry/recipe validation
and original-request graph/external coverage accept all 23 imported roots. Twenty-one distinct
chain-constructor variants yield 14 train, three validation and four test roots; two older
59-facility mixed-transport closure factories are out-of-family/scale evaluation only.
The policy is not trained on those two older factories. The small roots contain 4, 8 or 9
facilities, and variants share construction history. This is a narrow, correlated corpus.
The chain producer matrix also contains 116 stopped rows and 55 duplicate complete variants;
these remain recorded in the import report and are not counted as extra usable training roots.

Every sampled modification is online: generate a graph-derived single/connected-pair move,
repair affected paths pipe-first, validate geometry and original coverage, then update private
state. No future outcome table enters features. Current private score may worsen; only a
strictly better validated complete incumbent is returned. The score is always the exact tuple
`[used area, physical belt/pipe cells, directed turns including facility arms]`.

The bounded action space fixes rotations/port identities, uses symmetric axis translations
of one or two cells, and inherits a current-extent routing neighborhood. Whole-grid event-free
strip compaction is one explicit paid action. Lossy network summary tokens omit detailed route
occupancy, item/rate features and action history. These restrictions are experimental design
choices, not game rules, production W/H limits, or an exhaustive factory optimization domain.
In particular, moves and repaired routes must remain inside the current used extent and
compaction only contracts it. This environment does not test temporary area expansion followed
by repacking. Its proven detour worsens the secondary tile objective, not the bounding area.

## A learned path that requires temporary deterioration

The real citron root `chain-citron-51` has a two-action path:

```text
[152,20,9] -> [152,21,9] -> [133,20,10]
```

First move the connected pair `P1,S` one cell right; then move `G,P2` one cell right. Both
states remain complete valid factories. The first action adds a physical tile without reducing
area; the final action reduces area by 12.5%, despite one additional turn relative to the root.
Actions are derived from the general graph/action generator, not injected for this example.

The separate local audit exhausts the entire non-increasing reachable component, including
equal-score states: only one physical state is reachable and no improving transition exists.
There are no route-budget unknowns in that monotone audit. A nonmonotone search then finds
the path above. Thus deterioration is necessary **in this enumerated deterministic local
action/router system**, not in all legal game layouts, ports, routes or movements.

Train seven policies on this one training root: three terminal-credit seeds, three myopic
seeds and a zero-signal control, each with 512 groups of eight horizon-two trajectories
(8,192 online actions). Final-checkpoint evaluation allows only two proposals, at most two
independent validations and 24,000 total route expansions.

| Final policy/control | Fresh sampled improvements | Argmax improvements |
| --- | ---: | ---: |
| Terminal credit, seeds 7/17/29 | 96/96 | 3/3 |
| Myopic credit, seeds 7/17/29 | 0/96 | 0/3 |
| Zero signal, seed 7 | 0/32 | 0/1 |
| Uniform nonmonotone sampling | 0/32 | not applicable |
| Greedy / bounded best-first | 0/1 each | deterministic controls |

All terminal successes return `[133,20,10]`. In the final 1,024 training trajectories per seed,
terminal learners succeed 1,024/1,024, 1,024/1,024 and 1,023/1,024; all three myopic learners
succeed zero times, while the zero control succeeds three times. Learning was not supplied the
audit path. Still, it may memorize this same root; this is delayed-credit evidence, not unseen
factory generalization. Nonlearning search can also find the path with a larger budget.
The 8,192 training actions per policy and the separate exhaustive audit are not free: beating
a two-proposal baseline does not establish training amortization or global optimality.

The myopic control assigns rejected actions -1 and credits only immediate score changes;
terminal credit changes both the horizon and this immediate validity incentive. The control
is therefore not a pure discount-factor ablation. Peer-relative terminal preference also is
not a calibrated absolute global utility; see [research.md](research.md).

## General-root quality comparison

The table reports terminal policies' exact-score **wins / ties / losses** against each control.
Small-root cells contain four roots x three model seeds x three rollout seeds; large-root cells
contain two roots x three model seeds x three rollout seeds. Deterministic work-mode controls
are reused across the matching comparisons, not rerun or presented as independent evidence.
These are descriptive paired outcomes, not confidence intervals or tests on independent graphs.

| Roots / allowance | Versus uniform | Versus greedy | Versus bounded best-first |
| --- | ---: | ---: | ---: |
| Small test / 32 proposals | 11 / 11 / 14 | 3 / 11 / 22 | 3 / 11 / 22 |
| Small test / 128 proposals | 13 / 9 / 14 | 8 / 10 / 18 | 5 / 11 / 20 |
| Small test / 250 ms | 13 / 9 / 14 | 13 / 9 / 14 | 6 / 11 / 19 |
| Small test / 1,000 ms | 13 / 9 / 14 | 21 / 9 / 6 | 17 / 12 / 7 |
| Old large / 32 proposals | 4 / 9 / 5 | 0 / 0 / 18 | 0 / 0 / 18 |
| Old large / 128 proposals | 3 / 9 / 6 | 3 / 0 / 15 | 3 / 0 / 15 |
| Old large / 250 ms | 6 / 9 / 3 | 0 / 0 / 18 | 6 / 0 / 12 |
| Old large / 1,000 ms | 3 / 9 / 6 | 0 / 0 / 18 | 0 / 0 / 18 |

At one second on small roots, uniform sampling itself beats best-first in 7 of 12 comparisons,
ties three and loses two; the zero-signal neural policy has the same win/tie/loss count.
Thus the terminal model's 17 wins against best-first are not sufficient evidence of learned
superiority: stochastic nonmonotone exploration is already useful. At 128 proposals, terminal
policies improve the root in 21/36 small trials, versus 0/36 for myopic, 6/12 for zero,
6/12 for shuffled and 8/12 for uniform. Best-first improves three of four roots deterministically.
Different denominators and repeated geometry must not be hidden by pooling raw successes.

Per-root scores at 128 proposals illustrate why area alone is insufficient. The terminal column
is the median exact tuple of nine independent policy/rollout combinations, not a best-of-nine
portfolio given free extra work. All individual tuples/seeds remain in [results.json](../../../artifacts/benchmarks/layout-rl/results.json).

| Root | Initial tuple | Terminal median | Greedy | Best-first |
| --- | --- | --- | --- | --- |
| Battery, 8 facilities | 240 / 33 / 10 | 240 / 33 / 9 | 240 / 33 / 9 | 228 / 32 / 9 |
| Citron, 4 facilities | 176 / 19 / 6 | 176 / 19 / 6 | 176 / 19 / 6 | 176 / 19 / 6 |
| Iron, 9 facilities | 476 / 36 / 6 | 392 / 47 / 15 | 392 / 37 / 10 | 392 / 37 / 10 |
| Iron, 8 facilities | 330 / 35 / 6 | 286 / 49 / 21 | 286 / 35 / 8 | 286 / 35 / 8 |
| Old closure, original | 12900 / 2401 / 268 | 12900 / 2222 / 244 | 10912 / 2058 / 238 | 11625 / 2172 / 250 |
| Old closure, refined | 4032 / 879 / 193 | 4032 / 879 / 193 | 4032 / 872 / 195 | 4032 / 873 / 195 |

There are genuine individual gains: one terminal trajectory reaches `[336,65,21]` from the
nine-facility iron root, and another reaches `[9548,1769,236]` on the original closure factory.
Those are valid individual outcomes, not evidence that selecting the best seed for each root
would be a fair deployed policy. The refined closure root never improves under terminal,
zero, shuffled or uniform policies in this matrix, while both geometric controls shorten it.

The frozen main run has eight final policies, 65,536 online training actions each, horizon 16, width 32
and 16,065 parameters. It compares three terminal seeds with three myopic seeds, a zero-signal
control and a shuffled-terminal control. There is no test-based checkpoint/seed selection.
The held-out evaluation has 792 rows across the four small test roots and two old closure roots:
32/128-proposal work caps and 250/1,000 ms acceptance deadlines. Deterministic work controls
are not duplicated to inflate sample counts. Every candidate alternative consumes the same
per-proposal routing/validation allowance; actual consumed work is also reported.
Validation-curve rollouts add 1,920 actions to the main run outside those 524,288 training
actions. Root import/reset checks, final audits and checkpoint replay are additional disclosed
evidence work, outside the candidate allowance. Training CLI wall time includes initialization,
validation curves and artifact writes; the final main run takes 340.15 seconds.

In the final 1,024 training trajectories, terminal seeds 7/17/29 improve the initial root
38/112/93 times, versus 0/0/0 for myopic credit, 32 for zero and 32 for shuffled terminal credit.
Only 97/123/129 of 512 terminal groups contain differing final scores. Terminal trajectories
explore about 23,500 distinct observed state/action pairs per seed; myopic learners explore
3,200-3,500 and their final sampled 1,024 actions are all unchanged compactions. This confirms
a no-change policy in the myopic control, not high quality merely because its moves are valid.
All final argmax validation runs retain their three initial roots. Training improvement alone
therefore cannot establish held-out superiority. The zero control changes exactly zero weights;
the shuffled control changes weights without gaining comparable training success.

Main evaluation takes 401.10 seconds including post-run audits and artifact saves. No work-mode
30-second safety guard or time-mode 16,384-proposal safety cap is hit. In-flight work/reply
overrun above acceptance deadlines has median 0.55 ms and maximum 15.16 ms across all timed
rows, including controls that stop early. Forty routed path attempts exhaust their 12,000-
expansion shared-action allowance; these are retained as resource-limited failures, not
infeasibility. Most failed modifications instead identify shared terminal/anchor ownership,
terminal/body collision or the inherited neighborhood boundary. All failures preserve the best.

## CPU parallelism and cost

Host: Apple M4, ten physical/logical cores, 24 GiB RAM, macOS 26.6, Python 3.14.7 and CPU
PyTorch 2.14.0. No CUDA or MPS was used. Thread counts and independent environment workers
were varied separately; the learner and synchronous environment collection alternate phases.

| Environment workers | Small roots: 3,584 actions, seconds | Old large roots: 512 actions, seconds |
| ---: | ---: | ---: |
| 1 | 2.562 | 9.472 |
| 2 | 2.070 | 7.264 |
| 4 | 1.773 | 5.017 |
| 8 | 1.586 | 4.108 |

These include worker startup, all routing, serialization, immutable saves, independent final
audits and shutdown. The final small-root tail is not padded with fake work. Every action,
status, accepted score/witness and deterministic work count matches across worker counts.
Eight workers improve measured total throughput by 1.61x on small roots and 2.31x on large
roots, not 8x. Each cell is one fixed workload timing, not a confidence interval.

For the 59-facility observation (90 tokens, 1,041 actions), batch-eight median arithmetic costs
over 50 repetitions after five warmups are:

| PyTorch intra-op threads | Python tensor encoding, ms | Forward, ms | Gradient/update, ms |
| ---: | ---: | ---: | ---: |
| 1 | 18.70 | 2.382 | 6.410 |
| 2 | 18.34 | 1.533 | 4.342 |
| 4 | 18.36 | 1.248 | 3.448 |
| 8 | 18.66 | 1.396 | 3.911 |

Four threads were selected before the main quality run. More threads help matrix arithmetic
but do not remove the serial Python conversion cost. This cost probe uses real observations
but an entropy-gradient update; it is not an online quality or GPU result. The thread sweep
predates the deadline-only environment correction; its model/feature arithmetic is unchanged.
The environment-worker table is from the final corrected implementation.

A single process-tree RSS snapshot during the first complete main training run was 1,942,640
KiB across uv, the learner, eight Node workers and eight Rust probes; the Python process was
464,992 KiB. This is a sampled sum (shared pages counted repeatedly), not peak/exclusive RAM.
Cost-probe reports separately retain Python `ru_maxrss`. Full trajectory artifact storage is
also a cost, and is deliberately outside Git.
The final main training directory is about 1.1 GiB, targeted training 491 MiB, main evaluation
400 MiB and main replay audit 513 MiB (rounded filesystem `du` readings). Retaining full
geometry/trajectory evidence costs much more disk than the 1.46 MB summary, now kept
outside Git under `artifacts/benchmarks/layout-rl/results.json`.

## Verification, provenance and failed attempts

`b484ac2` adds only persistent independent validation transport and shared local route-work
accounting. `dca4461` adds the isolated full-factory environment. The accepted constructive
planner contract is preserved: production entry points are unchanged; every returned incumbent
is valid and improves strictly; private moves repair their routes atomically; no global exact
solver, game data hardcoding, port relaxation or W/H feasibility bound is introduced.

The first time-budget evaluation stopped after 237 saved episodes: it conflated the last 2 ms
of an episode with validator process health. That partial run is excluded from quality evidence.
The fix keeps the declared 3,000 ms health cap separate from acceptance; tests deterministically
cross the deadline and verify that late valid results are charged but cannot commit. Final
experiments are rerun with the same policy/action/reward/budget schedules. The seven detour
checkpoints and all eight main checkpoints are bit-identical to their predecessors. Earlier development runs and earlier
worker timing sweeps are not substituted for final evidence.

Final independent audits accept all 792 main and 266 targeted selected factories. They replay
8,161 main and 34 targeted transitions with identical accepted status, routed witness, score
and deterministic work. Restored JSON checkpoints reproduce all 6,144 main and 28 targeted
selected model decisions exactly. Time-budget trajectory branching is not replayed as if wall
time were deterministic; every timed final witness and its recorded work ledger is still audited.
The Rust probe alone deliberately does not declare original-request factory completeness:
the separate graph/external-coverage gate establishes that additional fact.

Rust formatting/check and workspace tests pass: 526 passed, two pre-existing ignored. The
selected Node suites (chain constructor, local re-embedding, earlier learner and this environment)
pass 170 tests; Python policy/protocol tests pass eight. CLI checks cover missing/duplicate
arguments, stale provenance and output reuse without overwriting. Tests also cover exact
terminal comparisons, zero-gradient weight stability, masked/padded model inputs, JSON
checkpoint round-trip, immutable roots, private deterioration, route accounts, late acceptance
and owned-process error/timeout cleanup.

Reproduction commands and artifact retention requirements are in [README.md](README.md).
The full fixed matrix and per-row results are retained, including failures and negative controls;
no global optimum or broad human-quality claim is made.

## Next research decision

Do not promote the present learner on the strength of the one-case detour. The useful completed
milestone is an online, validated, parallel environment with paid search controls and replayable
terminal-credit evidence. A next experiment should first separate the limited action/observation
domain from learning failure: consider bounded port/rotation or temporary-expansion moves,
route-occupancy/history observations, and more independently generated graph families with a
new untouched test split. These are follow-up proposals, not changes made after observing this
test set. The current data do not isolate which limitation dominates, and simply widening the
network or adding more CPU threads is not an evidence-backed solution.
