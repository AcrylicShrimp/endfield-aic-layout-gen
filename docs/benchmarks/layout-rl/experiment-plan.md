# Frozen experiment matrix

Status: final general-training and held-out evaluation schedules frozen before evaluation.

## Roots and support

The import plan yields 23 distinct complete generated roots: 21 newer chain-constructor
variants and two earlier mixed-transport closure variants. All 23 pass fresh Rust geometry
and original-request coverage, including every real external obligation. The small factories
have 4, 8 or 9 facilities; the earlier factories have 59. Original construction variants are
correlated and the two old factories share the same production graph; this is not 23 unrelated
factory-design problems or a broad recipe corpus.

Canonical rotation/translation-equivalent whole witnesses share one split. The deterministic
within-family split has 14 training, three validation and four test roots. Both old closure
roots are separate out-of-family/scale evaluation cases. No descendants are moved between
splits. The policy learns from newly generated trajectories, not community demonstrations.

## Development and mechanism experiment

`development.json` uses 64 groups, eight trajectories per group, horizon 16 and seed 7 for
terminal, myopic and zero controls. Its first run is exploratory evidence: the subsequent
Python import fix changed source identity, not the mathematical policy or environment. Do
not use it as the final quality experiment. Initial greedy validation decoding retained all
three validation roots; training improvement was sparse. This did not justify tuning on test.

The separate `detour.mjs` audit establishes an actual generated citron case with no outgoing
strictly improving or distinct equal-score state in the full screened action set. Its monotone
reachable component exhausts without route-budget unknowns. A validated two-action path changes
`[152,20,9] -> [152,21,9] -> [133,20,10]`. This is a proof about the stated local transition
system, not all game-legal movements, ports or routings. Both movements are connected-pair
translations proposed by the general action generator, not special actions added for this case.

`detour-training.json` trains on that **one training root**, horizon 2, 512 groups of eight,
three terminal seeds, three myopic seeds and one zero seed. `detour-evaluation.json` repeats
fresh two-proposal trajectories with 32 new sampling seeds and also checks deterministic
decoding. This tests delayed credit and may memorize the case; it is expressly not a held-out
generalization, amortization break-even or human-quality result. Exhaustive local proof work
and all policy training are paid separately from the two-proposal evaluation allowance.

## Main general-root experiment

`training.json` freezes 512 groups x eight trajectories x 16 modifications = 65,536 sampled
online transitions per run. Roots are sampled uniformly from all 14 training roots. Adam
learning rate 0.001, width 32, two directed message-passing blocks, two-head action attention,
gradient-norm cap 1, no entropy bonus/weight decay, and seeds 7/17/29 are fixed. Train terminal
and myopic controls at all three seeds, plus zero and shuffled-terminal controls at seed 7.
There is no best-test or best-seed checkpoint selection. Keep the final checkpoint of each run.

Terminal advantages compare complete final best-incumbent tuples against other independently
sampled trajectories in the same-root group, with ties zero and no discount. The myopic control
instead credits only the current action's lexicographic change (+1/0/-1), treats rejected
actions as -1, and centers that step's group signal. Thus it changes both credit horizon and
immediate validity incentive; do not claim a perfectly isolated discount-factor ablation.
The zero control must change exactly no weights. Shuffled advantages are permuted across
same-root trajectories on each update; they are not secretly corrected using final scores.

`evaluation.json` tests all final checkpoints and retain/uniform/greedy/best-first controls.
Learned policies are sampled because the trained objective concerns stochastic trajectories;
the mechanism experiment separately reports argmax decoding. Evaluation seeds 101/202/303
are fixed independently of training seeds. Work budgets are 32 and 128 proposals, with the
same maximum number of validations and 12,000 routing expansions per proposal. Work-mode
30-second limits are safety guards; report if any is hit. Deterministic work-mode controls
run once rather than count repeated identical outputs as new evidence.

Time-mode budgets are 250 and 1,000 ms with a 16,384-proposal safety ceiling. Every policy and
deterministic control is repeated for all three sampling IDs in this mode. Run jobs sequentially
in a fixed shuffled order to avoid systematic first/last-method timing bias. Include feature
generation, Python inference/encoding, IPC, routing and validation in the running episode
deadline. A late candidate never replaces the incumbent. Bounded in-flight work and reply
serialization may make return time exceed the acceptance deadline; report this overrun.
Startup/root revalidation and post-run audits are separate, disclosed costs, not free training.

Greedy tries actual routed candidates in cheap geometric order and takes the first strict
improvement. Best-first retains eight states, admits eight novel valid children per expansion
and allows nonmonotone states. It is a bounded heuristic, not an exact baseline. Every attempted
alternative consumes the same proposal/routing/validation accounts as a model-selected action.
Both keep every better validated incumbent they encounter, including one found at budget end.

## CPU matrix and reporting

The development host has ten physical/logical CPU cores and 24 GiB RAM. Measure PyTorch
threads 1/2/4/8 on actual small- and large-root observations, separating Python tensor encoding,
forward inference and gradient-update arithmetic. Fifty timed repetitions follow warmup.
An initial overlapping timing is exploratory only; the final thread sweep runs sequentially.
Four threads have the best measured arithmetic in that sweep and are chosen for the main run.

Measure environment workers 1/2/4/8 on the same 56 small-root and 16 large-root seeded
trajectories. Each case/seed retains identical actions, statuses, scores and deterministic work
across worker counts; wall times and output paths are not expected to match. Learner computation
and synchronous environment collection happen in alternating phases, not eight workers each
starting four PyTorch threads. Report complete CLI timing plus work/feature/probe costs.

## Infrastructure correction after freezing the matrix

The first main evaluation stopped after 237 saved episodes (the last progress line was 224)
because an episode's last 2 ms
were incorrectly used as the validator process-health timeout. Its incomplete output is not
quality evidence. The correction keeps the declared 3,000 ms in-flight validation cap separate
from the episode acceptance deadline; a candidate returned after the deadline is still rejected.
This changes no action, routing domain, model, reward, seed, training schedule or evaluation cap.
Regression tests inject the deadline boundary without flaky sleeps. Repeat training, worker
measurements and evaluation under the corrected source identity. Record checkpoint equality
against the preceding runs, and disclose bounded return-time overrun instead of promising
that a 250 ms acceptance cutoff also guarantees a 250 ms process return.

Report every per-root score, model and sampling seed, negative control, failed modification,
unknown, shared work count and fresh final coverage audit. Exact score tuples, not training
reward alone, determine whether learned search beats each nonlearning control. The small
held-out root count limits inference regardless of the number of generated transitions.
