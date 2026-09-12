# Existing large-factory success learning diagnostic

## Decision

The five successful training edits were repeatedly given positive SIL weight,
but the originating final policies still rarely select their successful poses.
This is more specific than missing exposure or universally disabled imitation.
Two recently discovered cases improve their exact pose probability, while three
older cases lose it. Do not treat the reuse counts as evidence of mastery, or
an inability to memorize all five as already proved.

At two late saved states, these rare entries' expected combined SIL gradient
is only 0.159% and 0.0192% of the originating PPO actor gradient. This supports
investigating replay composition and relative loss influence before extending
the same training schedule. It is a local measurement, not a causal attribution
of the whole training history. No additional training was needed or performed.

## Scope and continuity

The [preceding diagnosis](2026-09-13-blueprint-large-factory-bottlenecks.md)
identified five native-validated large training successes in standardized seeds
17 and 29. This probe uses their exact 15 retained pose/repair/commit label states.
It inspects all six policies at updates 0/128/256/384/512 and originating policies
at retained admission-adjacent checkpoints: 33 checkpoints and 495 predictions.
Only two originating arms' update JSONs and referenced compressed chunks are read.

Recorded candidate lists and tensors are preserved, including actual episode
progress and elapsed features. A one-time check reproduces each selected input
from its saved observation under the bound source. No factories are regenerated,
no native action executes, and no evaluation case enters fitting. These are
conditional predictions on recorded successful states, not autonomous rollouts
from original factories, trajectory success rates, or generalization scores.

The two backward-only probes restore update 511 and inspect update 512 without
an optimizer step. They reuse the frozen packed graph model/distribution and
64-row minibatch size, on one CPU thread. No training path is replaced or enabled.
The user's requirement to use the established optimized path for any additional
training remains applicable. Earlier worker-migration obligations remain open;
this diagnostic does not claim to close them.

## Historical SIL was active on these examples

For each recorded SIL minibatch, selected IDs and aggregate positive-label counts
are retained. If every label in a minibatch has positive weight, every selected
success's labels do too. Mixed minibatches cannot attribute their zero weights
to individual entries, so the following are lower bounds, not estimates.

| Origin / input slot | Reuses | All labels provably positive | Mixed, unattributed | Positive-use lower bound |
| --- | ---: | ---: | ---: | ---: |
| Standardize 17 / 152 | 975 | 922 | 53 | 94.56% |
| Standardize 17 / 1665 | 44 | 40 | 4 | 90.91% |
| Standardize 17 / 1666 | 38 | 37 | 1 | 97.37% |
| Standardize 29 / 509 | 1144 | 1082 | 62 | 94.58% |
| Standardize 29 / 1179 | 395 | 370 | 25 | 93.67% |

Together, 2,451 of 2,596 sequence uses definitely gave all three labels positive
`max(return_to_terminal - detached_value, 0)` weight. None appears in an
all-zero minibatch. The remaining 145 uses are unresolved at per-label level.
Positive weight establishes an active imitation loss term, not a guaranteed
net probability increase after other losses, clipping and Adam.

All 15 labels also have positive originating PPO actor advantage, reconstructed
with hashes matching the recorded advantage and target tensors. Immediate PPO
credit with the wrong sign does not explain these particular successful labels.
At the final originating checkpoints every label still has positive SIL weight.
Initial untrained checkpoints do have zero weights because their initial value
predictions exceed the returns; those snapshots precede admission and are not
evidence that later replay was inactive.

## The exact successful poses remain unlikely

Probabilities below use each example's originating learner. Discovery probability
is the recorded behavior probability when the successful pose was first sampled.

| Input slot | Discovery pose probability | Final pose probability | Final / discovery | Final rank / candidates |
| --- | ---: | ---: | ---: | ---: |
| 152 | 0.029088% | 0.002383% | 0.082 | 117 / 791 |
| 1665 | 0.004027% | 0.040654% | 10.095 | 21 / 414 |
| 1666 | 0.002978% | 0.014942% | 5.017 | 48 / 513 |
| 509 | 0.023279% | 0.002828% | 0.121 | 726 / 979 |
| 1179 | 0.032375% | 0.018907% | 0.584 | 290 / 404 |

The two late cases admitted at update 418 improve, although neither becomes a
high-probability action. The medical case reused 1,144 times falls about eightfold
from its discovery probability. More exposure therefore has not monotonically
improved the action probability under the existing mixed objective.

Separate the probability of choosing any pose from choosing the correct pose
conditional on that action kind:

| Input slot | Any pose, final | Correct pose given pose, admission-adjacent | Correct pose given pose, final |
| --- | ---: | ---: | ---: |
| 152 | 0.797% | 0.193% | 0.299% |
| 1665 | 2.745% | 0.397% | 1.481% |
| 1666 | 1.438% | 0.580% | 1.039% |
| 509 | 3.196% | 0.135% | 0.088% |
| 1179 | 8.272% | 0.281% | 0.229% |

For slot 152, discrimination within poses improves while total pose probability
falls. Slots 509 and 1179 also lose discrimination within that kind. A single
explanation such as never learning specific destinations is insufficient.

In originating final models, repair-all probabilities at recorded repair states
are 33.47--37.44%, and commit probabilities at commit states are 35.69--46.25%.
The centered policies exhibit the opposite imbalance on the same 15 states:
pose mass remains 85.52--94.75%, including after useful routing is finished.
Across centered final policies and five cases, exact repair-all probability is
0.0217--2.1521%, and commit probability is 0.2926--5.3346%.

These centered policies did not receive these standardized success labels in
their own SIL buffers. Their predictions measure transfer to these recorded
states, not failed memorization of supplied labels. No final policy ranks any
of the five successful pose labels first. A singleton repair action may rank
first while the aggregate probability of hundreds of poses remains near 90%;
argmax correctness must not be confused with stochastic execution reliability.

## Expected late SIL influence is small

At the saved start of update 512, compute the full 256-row PPO gradient by
accumulating the existing 64-row packed batches. Include policy surrogate,
entropy and value-loss gradients on actor-owned parameters: the value head
shares the encoder, so its encoder gradient belongs in this comparison.

For a retained success entry with stages `S` and buffer population `N`, its
expected contribution under uniform sequence sampling is:

```text
0.05 / N * mean_stage(mean_labels_in_stage(-log_probability * positive_weight))
```

Sampling eight sequences with replacement and averaging them has this same
expectation. Measure the sum for the three selected seed-17 entries and the two
selected seed-29 entries. This is their expected contribution, not all SIL.

| Origin | Buffer population before update | PPO actor gradient norm | Rare expected SIL gradient norm | Norm ratio | Cosine |
| --- | ---: | ---: | ---: | ---: | ---: |
| Standardize 17 | 249 | 0.067771 | 0.00010775 | 0.1590% | 0.0232 |
| Standardize 29 | 104 | 0.150887 | 0.00002893 | 0.0192% | 0.0678 |

The population before seed-17's last update is 249, versus 250 at completion;
the diagnostic uses the correct pre-update population. These ratios are
pre-clipping, pre-Adam expectations at two points. They cannot identify the
accumulated optimizer displacement or imply that increasing the coefficient by
their reciprocals is appropriate. The near-orthogonal cosines do not establish
direct cancellation by PPO at these points.

Within rare SIL, aggregated pose gradients have negative cosine with commit
and repair gradients: seed 17, -0.491/-0.440; seed 29, -0.791/-0.796. Shared
softmax action competition can produce this without a bug. This flags phase
discrimination as worth testing, not proof that the representation cannot learn.

## What to do next

Continue with a bounded comparison of replay composition and relative imitation
weight, measuring the exact pose and completion actions separately. The existing
five examples may serve as explicitly labeled diagnostic anchors; they are not
new demonstrations. Any fitting must use the established optimized execution
path and preserve the fresh-factory contract for normal training. A successful
diagnostic fit would still need independent large-factory transfer evaluation.

Do not choose longer training solely from exposure counts or aggregate small-
factory improvement. Missing large successes in centered replay, weak expected
rare-example influence in standardized replay, and persistent action-kind
imbalance are now concrete observations. Capacity failure, global inability to
improve large factories, and a single causal loss bug remain unproved.

## Reproduction and verification

Copy the committed script into the frozen advantage experiment root and run:

```sh
"$PILOT_PYTHON" tools/factory-edit-pilot/diagnose_blueprint_success_learning.py \
  --matrix artifacts/factory-edit-pilot/advantage-main \
  --selection artifacts/factory-edit-pilot/bottlenecks-20260913/rare-large-successes.json \
  --output artifacts/factory-edit-pilot/success-learning-20260913/final.json
```

The output must be fresh. The executed copy is retained under the frozen root's
`artifacts/factory-edit-pilot/bottlenecks-20260913/` directory. The compact
[provenance record](2026-09-13-blueprint-success-learning-provenance.json)
binds its source, selected inputs and final diagnostic output. Local raw evidence
is ignored under `artifacts/factory-edit-pilot/blueprint-success-learning-20260913`.

The actual CLI passed source bindings, recorded-input reproduction, native
witness presence, source PPO credit hashes, exact historical use totals, model
identities, normalized distributions and pre-update behavior/value agreement.
The gradient implementation includes the shared encoder's value-loss term;
an earlier exploratory calculation omitted it and was superseded. A source-name
shadowing error introduced during binding checks was caught by empty-batch
rejection, fixed, and the complete final CLI rerun passed. No optimizer ran.

Workspace check and tests passed (564 passed, 5 ignored), followed by worktree
and index artifact-policy checks. No Rust or production training behavior changed.
