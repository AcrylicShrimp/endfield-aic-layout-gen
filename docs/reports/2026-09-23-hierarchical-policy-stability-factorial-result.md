# Hierarchical policy stability factorial: eight-generation result

## Question and verdict

Does successful-episode root replay, exclusion of failed-episode policy targets,
or their combination reduce the outer-policy case churn diagnosed in the
stopped 88-generation curriculum?

**Successful-root replay has the clearest positive signal in this one-seed
screen.** It improves the final two diagnostic evaluations whether failed-root
policy targets are retained (B over A) or excluded (D over C). Excluding
failed-root policy targets has an inconsistent effect: C exceeds A, but D does
not exceed B. D's aggregate diagnostic count rises at every evaluation, yet
individual cases continue to be lost. This screen identifies a promising
mechanism, not a selected production correction. No ultra-long training is
justified by one stream and one diagnostic panel.

The [predeclared design](../designs/2026-09-23-hierarchical-policy-stability-factorial.md),
[smoke closeout](2026-09-23-hierarchical-policy-stability-smoke.md), and
[27/64 common-start baseline](2026-09-23-hierarchical-policy-stability-baseline.md)
preceded this screen. The historical 128-case validation panel was not read.

## Controlled comparison

All arms started from the same generation-87 lower and outer weights. Each
trained on the same 32 fresh two-facility problems per generation, with the
same eager Gumbel search, 200 epochs, 256 outer root-state slots per epoch,
16-state batches and 3,200 optimizer updates. The lower model remained
byte-identical throughout. B and D replaced 128 outer slots per epoch with
validated successful roots from the previous four committed generations once
available. A and B trained policy targets from failed roots; C and D removed
only those policy terms while retaining failed-root value supervision and the
original full-batch policy-loss denominator. All four arms used the same new
64-case diagnostic panel at generations 1, 3, 5 and 7.

| Arm | Replay | Failed policy targets | Gen 1 | Gen 3 | Gen 5 | Gen 7 | Last-two mean | Last-two minimum |
| --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| A | Off | On | 30 | 27 | 24 | 21 | 22.5 | 21 |
| B | On | On | 25 | 23 | 33 | 34 | 33.5 | 33 |
| C | Off | Off | 22 | 31 | 26 | 29 | 27.5 | 26 |
| D | On | Off | 25 | 27 | 31 | 34 | 32.5 | 31 |

Counts are completed layouts out of 64. The final-two replay gains are +11
for B over A and +5 for D over C. The failed-target exclusion contrasts are
+5 for C over A and -1 for D against B. The 2×2 interaction on the final-two
mean is -6 completions. These are descriptive differences under one panel
stream, not uncertainty-adjusted effects.

At generation 7, B solves 18 cases A fails and loses 5 cases A solves. D solves
16 cases C fails and loses 11 cases C solves. Relative to the common start, B
gains 15 cases and loses 8; D gains 20 and loses 13. The last transition still
has churn: B gains 12 and loses 11 cases between generations 5 and 7; D gains
12 and loses 9. D's 25→27→31→34 aggregate improvement is therefore real but
does not mean each solved problem stayed solved.

The six-connection stratum at generation 7 is A 2/10, B 9/10, C 6/10 and D
7/10, versus 4/10 at the common start. The three-connection stratum is A 6/6,
B 4/6, C 6/6 and D 5/6, versus 5/6 at the start. Thus B's stronger final-two
minimum comes with a small easy-stratum regression. Every diagnostic failure
at the last evaluation terminates at `no-macro-actions`; these are bounded
search failures, not global infeasibility or invalid published layouts.
Because the panel contains exactly two-facility problems, it gives no evidence
about transfer to three or more facilities.

## Mechanism and runtime checks

All 32 generation reports have 3,200 updates and the original lower-checkpoint
SHA-256. In replay arms, the prior-success pool is populated from generation 1
onward and exactly 128 of 256 outer slots per epoch use it; it grows from 147
roots after generation 0 to 589 in B and 618 in D before generation 7.
Training collection roots and failed-root policy counts match the declared
treatments. No arm changed its panel stream, search configuration or facility
level. The completed W&B histories were acknowledged through step 7 for
[A](https://wandb.ai/acrylicshrimp-personal/endfield-aic-layout-gen/runs/ee78cfa06405cde238aed635),
[B](https://wandb.ai/acrylicshrimp-personal/endfield-aic-layout-gen/runs/cfc34fb4e9e5220d6b507ccd),
[C](https://wandb.ai/acrylicshrimp-personal/endfield-aic-layout-gen/runs/42b17b0e7c348ff8bb061de8),
and [D](https://wandb.ai/acrylicshrimp-personal/endfield-aic-layout-gen/runs/2266309dfe31a428e52273ba).

The final outer policy-head relative L2 movement from the common start is
0.177, 0.190, 0.150 and 0.186 for A through D. Failed-root search targets
still place roughly 0.71-0.73 mean mass on unvisited actions across arms;
removing their policy loss does not make the search teacher itself precise.
Integrated arm times, including collection, storage, validation, training and
diagnostic evaluation, are 27.5, 27.4, 28.9 and 28.2 minutes, respectively
(112.1 minutes total). Conditional layout quality is recorded in the evidence
but is secondary to completion and differs over changing sets of solved cases.

## Decision and limits

The retention hypothesis is supported as a **screen-level signal**. The
failed-target hypothesis alone is not established, and the combination is not
uniquely required. B has the highest final-two mean and minimum, while D has a
steadily rising aggregate and better preservation of the simplest connection
stratum at the final evaluation. Neither is a defensible production choice from
one seed. Both still replace previously solved cases, and the lower learner was
deliberately frozen here.

Before selecting a treatment, repeat the replay contrasts on at least three
predeclared training-panel streams from the same start, with paired case
analysis and an explicit easy-stratum/retention threshold. Then test the
promising choice with lower training restored and an untouched final held-out
panel. The numeric long-run gate must be fixed before those confirmation
outcomes are seen. This is a research decision gate, not an ultra-long launch.

## Evidence and reproduction

The compact [screen evidence](2026-09-23-hierarchical-policy-stability-screen-evidence.json)
contains per-case four-evaluation bit masks, paired gains/losses, connection
strata, root entropy and maximum prior, policy-target mass on unvisited
actions, replay and policy-root counts, parameter movement, wall time, input,
report and checkpoint hashes, and W&B acknowledgements. Bit 0 through bit 3
of each hexadecimal mask correspond to generations 1, 3, 5 and 7 in the
listed diagnostic-case order. The exact training source is commit `31555f5`;
the server release binary hash is in the
[launch manifest](2026-09-23-hierarchical-policy-stability-screen-launch.json).
Source configurations and panel-selection code are tracked under
`docs/benchmarks/hierarchical-policy-stability/`; the bounded sequential
runner and analyzer are under `tools/factory-edit-pilot/`.

From `~/Srv/aic-policy-stability-20260923`, with the retained PyTorch virtual
environment and the prepared input paths in the launch record, regenerate the
compact evidence before raw cleanup with:

```bash
"$HOME/Srv/aic-rust-learning-20260915/.venv/bin/python" \
  tools/factory-edit-pilot/analyze_hierarchical_stability_screen.py \
  --screen artifacts/policy-stability-screen \
  --inputs artifacts/policy-stability-inputs \
  --output artifacts/policy-stability-screen/evidence.json
```

The raw collection files, generated panels, intermediate optimizer
checkpoints and final model checkpoints have no active continuation
requirement. They are removed after verifying the committed closeout evidence;
the cleanup manifest records exact paths and free-space change.
