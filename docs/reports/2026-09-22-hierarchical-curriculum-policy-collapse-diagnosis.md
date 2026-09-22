# Hierarchical curriculum policy-collapse diagnosis

## Question and verdict

The stopped curriculum run regressed while it remained at the first complexity
level. This diagnosis asked which learned component caused the decline and why
MCTS distillation reinforced it instead of correcting it.

**Verdict: the outer policy prior collapsed.** The lower routing model improved,
and the outer value heads were not the cause. By generation nine the learned
outer prior concentrated on a small, harmful action subset. Progressive
widening admitted only the highest-prior prefix, so MCTS often could not examine
the actions that recovered the case. The learner then imitated those biased
visit counts from successful and failed episodes alike. Training on only the
current 32-case generation made that error self-reinforcing.

Do not resume the original run. The next learner must first pass the retained
diagnostic panel with outcome-aware policy targets, bounded replay and an
exploration rule that guarantees anchor-ranked alternatives enter the tree.

## Controlled panel and method

The fixed 32-case
[`diagnostic panel`](../benchmarks/hierarchical-curriculum-policy-collapse/panel.json)
contains only training cases and does not use the held-out evaluation set. Its
[`manifest`](../benchmarks/hierarchical-curriculum-policy-collapse/panel-manifest.json)
selects eight two-connection controls that generation zero solved, all 17
multi-connection successes from generations zero through four, and seven
generation-zero multi-connection failures. This deliberately measures retained
capability and recovery potential rather than estimating generalization.

The panel SHA-256 is
`976bceeb93bf935578e862ce6d715184f6f3be939c3d736854ba74a56071d873`;
the manifest SHA-256 is
`0e13d484c91f838c7e81188bf14618a6a87260c96a3a8ad7d80c0979c5e0f12b`.
All comparisons used source commit
`5e8cbf4d6d45425ce916b597fc1961400868c292`, the canonical search
configuration, CUDA and identical panel order. The compact machine-readable
results are retained in
[`policy-collapse evidence`](2026-09-22-hierarchical-curriculum-policy-collapse-evidence.json).

A representative matrix cell is reproduced on the training server with:

```bash
target/release/aic-cli learning hierarchical-search-harness \
  --config docs/benchmarks/hierarchical-curriculum/search.json \
  --panel docs/benchmarks/hierarchical-curriculum-policy-collapse/panel.json \
  --lower-training-output artifacts/hierarchical-curriculum-v1/generations/000009/training/lower \
  --outer-training-output artifacts/hierarchical-curriculum-v1/generations/000009/training/outer \
  --catalog-root data/game/normalized \
  --output artifacts/hierarchical-curriculum-diagnosis-20260922/lower-g9_outer-g9.json \
  --device cuda:0
```

Substitute the retained warm, generation-zero or generation-four checkpoint
paths to reproduce the crossed cells. Report and checkpoint hashes in the
evidence file bind every summarized result to its exact inputs.

## Component isolation

The matched checkpoints reproduce the decline: warm and generation zero solve
20/32, generation four solves 18/32, and generations nine and ten solve 9/32.
Crossing lower and outer checkpoints identifies the failing component.

| Lower checkpoint | Outer checkpoint | Completed |
| --- | --- | ---: |
| Warm | Warm | 20/32 |
| Generation 0 | Generation 0 | 20/32 |
| Generation 4 | Generation 4 | 18/32 |
| Generation 9 | Generation 9 | 9/32 |
| Generation 10 | Generation 10 | 9/32 |
| Warm | Generation 9 | 10/32 |
| Generation 0 | Generation 9 | 9/32 |
| Generation 4 | Generation 9 | 9/32 |
| Generation 9 | Warm | 24/32 |
| Generation 9 | Generation 4 | 18/32 |

The generation-nine lower model raises the warm outer result from 20 to 24,
while the generation-nine outer model reduces every tested lower checkpoint to
9 or 10. The lower model therefore retained and improved useful routes; it is
not the source of the regression.

Outer checkpoint ablations use the generation-nine lower model:

| Generation-nine outer variant | Completed | Interpretation |
| --- | ---: | --- |
| Learned policy and value | 9/32 | Reproduces the failure |
| Value heads zeroed | 9/32 | Value prediction is not causal |
| Policy heads zeroed | 30/32 | Removing learned action preference recovers search |
| Anchor prior mass 1.0 | 30/32 | The deterministic anchor ranking also recovers search |
| Anchor 1.0 and values zeroed | 30/32 | Recovery does not depend on learned value |
| Anchor prior mass 0.50 | 9/32 | Probability mixing is insufficient |
| Anchor prior mass 0.75 | 10/32 | A residual learned ranking still blocks recovery |

These interventions alter which actions can enter the tree. They do not retrain
the models or change the lower candidate set, so the 21-case recovery from 9 to
30 directly implicates outer-policy ordering.

## How the collapse operates

At the root, generation nine has a mean 224 macro actions. Its learned prior
puts mean probability `0.6007` on one action and `0.7448` on the top four; mean
normalized entropy is `0.4386`. The warm prior has top-one probability `0.0100`
and entropy `0.9401`; the anchor prior is almost uniform, with top-one
probability `0.0047` and entropy `1.0000`.

With 64 simulations, the current progressive-widening rule admits only
`ceil(sqrt(parent visits))`, approximately eight root actions, in descending
prior order. Across the 21 cases recovered by the anchor, the successful first
action is never the same first action chosen by the learned policy. Its median
rank under the learned prior is 18, so it is normally ineligible for selection.
Raising the budget on eight recovered cases helps too little: 64 simulations
solve 0/8, 256 solve 2/8, and 1,024 solve 3/8. The same restriction recurs at
deeper nodes; a larger budget is not an efficient correction.

Collection then closes the feedback loop:

1. A small learned bias determines the prefix that progressive widening admits.
2. MCTS visits only that prefix and selects from it, even when the episode later
   terminates without a complete layout.
3. Every recorded root receives a cross-entropy target made from its visit
   counts. Episode outcome affects value targets but does not qualify the policy
   target.
4. The current generation's records replace the preceding training set and are
   fitted for 200 epochs, sharpening the same preference.
5. The next collection exposes still fewer useful multi-connection actions.

The fraction of selected actions equal to the prior top choice rises from
`0.508` in generation zero to `0.994` in generation nine. Over the same span,
roots from failed episodes rise from roughly one half to `0.636` in generation
nine. Those failed roots are still used as ordinary visit-imitation examples.
The model learned its supplied teacher signal strongly; that signal ceased to
be a reliable policy improvement target.

## Relation to AlphaZero-style training

Canonical AlphaZero also trains its policy on MCTS visit distributions from
positions in games that are eventually lost. Discarding all failed policy
targets is therefore not the defining difference. AlphaZero makes the visit
distribution a stronger teacher through several coupled conditions: PUCT can
select any legal action rather than a hard prior-ranked prefix, root Dirichlet
noise perturbs exploration, early moves sample from visit counts, many more
simulations refine each root, a large recent-game replay buffer mixes training
states, and the final game outcome supplies the value target.

This solver violates the exploration condition that makes unconditional visit
imitation tolerable. At a 224-action root, the learned policy can keep a useful
action outside the eligible set, so value search cannot overturn the prior no
matter how accurate the value is. There is also no replay buffer to retain
earlier hard successes. The corrective design should copy the relevant
properties rather than copy AlphaZero's loss in isolation.

## Requirements before another curriculum run

The smallest complete treatment is:

1. Qualify failed-episode policy targets by later completion or an explicit
   outcome/advantage weight. Preserve value supervision from failures, but do
   not let an unsuccessful, policy-censored search act as an equally trusted
   policy teacher.
2. Train from a bounded four-generation or 128-case replay window, stratified so
   successful multi-connection roots remain represented.
3. Make progressive widening admit a bounded mixture of learned-prior and
   anchor-ranked actions. Mixing probabilities alone is inadequate because
   anchor masses of 0.50 and 0.75 did not change the harmful eligible prefix.
4. Record policy entropy, prior top mass and anchor disagreement. Stop or reject
   a promotion when the fixed diagnostic panel regresses or policy
   concentration crosses a calibrated guard.

Before a long run, execute a short readiness series on fresh training panels and
require that the fixed diagnostic panel retains its hard successes. Continue to
exclude the original held-out panel from checkpoint selection; use it only for
the final generalization measurement.

The subsequent
[`search-budget diagnosis`](2026-09-22-hierarchical-search-budget-diagnosis.md)
found an unchanged-search knee at 1,024 simulations and no additional completion
at 2,048. Budget scaling alone is therefore rejected; the next comparison must
change candidate allocation while keeping the fixed panel and checkpoint
weights constant.

The subsequent
[`adapted Gumbel root search`](2026-09-22-adapted-gumbel-root-search.md)
implements that allocator. Its selected 128-simulation treatment completes
27/32 fixed cases, including 4/7 early hard failures, while the unchanged PUCT
search completes 19/32 at 1,024 simulations and none of the seven failures.

## Retention state

The original stopped run remains recoverable at generation 11 and retains the
checkpoints needed to implement and compare a corrected learner. The diagnosis
retains the compact evidence, panel, manifest, exact run commands and ablation
configurations. After every report hash was verified against the evidence file,
large per-case reports and disposable cloned checkpoints were removed.
