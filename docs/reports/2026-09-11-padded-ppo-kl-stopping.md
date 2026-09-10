# Why padded PPO increasingly stops at the KL threshold

The records do not support a sudden increase in unusually large area rewards as
the simple explanation. KL stopping becomes frequent while raw advantages shrink,
the policy becomes more concentrated and pre-clipping actor gradients grow. The
learner still applies about 32 Adam steps at a fixed learning rate before its
first KL check. These observations explain the mechanism and plausible sources
of sensitivity; they do not isolate each factor's causal contribution.

The active twelve-hour run is unchanged. This follow-up reads the committed
prefix through batch 144 and reconstructs GAE from retained traces on CPU.

## Comparable first-epoch diagnostics

Values are means over twenty training batches. Gradient values are each batch's
maximum actor gradient norm in epoch one, before clipping; they are not actual
parameter-step magnitudes. Factory mixtures differ between windows.

| Measurement | Batches 1-20 | Batches 125-144 |
| --- | ---: | ---: |
| Episode area reward | 0.4160 | 0.4808 |
| Raw advantage standard deviation | 0.04180 | 0.01286 |
| Training entropy during first epoch | 3.946 | 1.951 |
| Actor gradient maximum before clipping | 1.816 | 6.195 |
| Adam steps before first KL check | 31.70 | 31.85 |
| Approximate KL after first epoch | 0.01037 | 0.02723 |
| Selected-action ratios outside 0.8-1.2 | 13.31% | 22.94% |
| Updates finishing after one epoch | 0 / 20 | 18 / 20 |
| KL-stopped updates | 4 / 20 | 20 / 20 |

The configured learning rate remains 0.0003, target KL 0.02, gradient norm limit
0.5, entropy coefficient 0.001 and maximum four epochs. The KL estimate compares
the new policy to the policy that collected that batch's actions. It is not a
comparison to the experiment's initial policy or a percentage of area improvement.

## Reward is not advantage

The actor is trained on GAE, which compares rewards and subsequent value estimates
against the critic's prior prediction. The update then normalizes advantages over
the whole collected batch: `(A - mean(A)) / std(A)`.

As prediction improves, smaller raw residuals do not automatically make the actor
update weaker: normalization restores unit variance. A smaller denominator alone
does not imply numerical instability or larger typical normalized advantages.
The relative size of unusual residuals and the model's gradients also matter.

Retained batches 1, 4, 16, 64 and 128 reconstruct the recorded raw advantage
mean/std within explicit floating-point tolerances. In batch 128:

- The largest absolute normalized advantage is **-8.25**, on an accepted `pose`
  with no immediate area gain. Its critic value was 0.19770 and raw GAE -0.09852.
- Another `pose` reduces area by **7.69%** but has normalized advantage **-8.15**.
  The critic value was 0.19179 and raw GAE -0.09748: positive immediate reward
  still fell short of the GAE/value expectation.
- A physically unchanged `restore_best` has normalized advantage **-8.02**.
- The largest positive normalized advantage is **+6.28** on `contract`.

The top 1% by absolute normalized advantage accounts for 23.27% of squared
advantage mass in batch 128, versus 12.59% in batch 1. This describes the signal's
tails; it is not a measured share of policy-gradient norm or KL.

Both batches contain 2,048 actions. The maximum single-step area reward is 0.667
in both; the count of actions gaining at least 25% of initial area is 58 initially
and 51 at batch 128. The recent batch therefore did not newly encounter larger
or more numerous rewards under this descriptive threshold. Its strongest
normalized residuals are negative disappointments, not only positive surprises.

## Why clipping does not prevent the observed KL

`ppo_training_update.py` checks KL after a complete epoch. One epoch currently
contains roughly 31-32 minibatches, each applying one Adam step. A one-epoch stop
therefore happens after many parameter updates. The completed epoch remains
applied; the test suppresses additional epochs rather than rolling back that one.

PPO clipping changes the sampled surrogate objective; it does not clamp every
new action probability to within 20% of its old value. Shared parameters change
probabilities across observations, including samples not currently in the
minibatch. The gradient-norm limit also does not directly bound KL or the Adam
parameter-step norm. Ratios outside the clipping range are consequently not, by
themselves, a clipping implementation bug.

Lower entropy documents a different policy regime, but lower entropy alone does
not mathematically guarantee a larger KL for the same parameter perturbation.
The measured increase in pre-clipping gradients and epoch-end KL is separate
evidence. A controlled replay would be needed to distinguish learning-rate,
normalization, state distribution, model sensitivity and optimizer-history effects.
Current epoch-level records cannot identify which exact minibatch crossed 0.02.

The supported reading is that the unchanged update schedule now consumes its
policy-change allowance much sooner. Frequent stops are the configured guard
working; they do not establish improved factory quality or catastrophic policy
collapse. The contemporaneous [action analysis](2026-09-11-padded-ppo-progress-batch-102.md)
shows strong early compression and weak additional improvement later in episodes.

## Reproduction and verification

```sh
python tools/factory-edit-pilot/profile_ppo_kl.py --run "$RUN" --through 144 --window 20 --trace-batches 1 4 16 64 128 --output "$FRESH_OUTPUT"
```

The diagnostic freezes the state snapshot, checks record hashes and reconstructs
GAE directly from recorded rewards and old values. It performs no policy forward
pass, optimizer step or GPU work. Two new focused tests verify negative advantage
despite positive rewards and credit assigned through a zero-reward action. All
seven related analysis tests pass; workspace and artifact-policy checks pass.
The [provenance](2026-09-11-padded-ppo-kl-stopping-provenance.json) identifies
retained raw evidence and source hashes. No active training source or setting changed.
