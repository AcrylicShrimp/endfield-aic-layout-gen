# Hierarchical policy stability: two-generation smoke closeout

## Question and method

Can the predeclared two-factor comparison preserve identical inputs and update
budgets while toggling successful-root replay and failed-root policy targets,
and can a replay-bearing generation resume from an optimizer checkpoint?

This smoke ran arms A (no replay; failed-root policy targets retained) and D
(four-generation successful-root replay; failed-root policy targets removed)
from the same generation-87 lower and outer model weights. Both used eager
search, 32 fresh two-facility cases per generation, 256 outer root-state slots
per epoch, 16-state batches, 200 epochs, frozen lower weights, and the same
fixed 64-case train-source diagnostic panel. That panel was filtered against
all eight predeclared training panels by canonical problem identity. The older
128-case validation panel was not used. The experiment design and exact source
configurations are in
[`../designs/2026-09-23-hierarchical-policy-stability-factorial.md`](../designs/2026-09-23-hierarchical-policy-stability-factorial.md)
and `docs/benchmarks/hierarchical-policy-stability/`.

The D process was deliberately terminated after the generation-1 epoch-20
checkpoint, then resumed with `--resume`. The A process was restarted during
generation-1 search after its generation-0 commit. No interrupted output was
accepted as a completed generation. Both W&B observers independently mirrored
the two committed generations and the remote API acknowledged history step 1.

## Result and verdict

| Arm | Generation | Training completed | Outer roots | Replay roots available | Updates | Diagnostic completed |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| A | 0 | 18/32 | 270 | 0 | 3,200 | not evaluated |
| D | 0 | 18/32 | 270 | 0 | 3,200 | not evaluated |
| A | 1 | 15/32 | 285 | 0 | 3,200 | 22/64 |
| D | 1 | 15/32 | 282 | 147 | 3,200 | 29/64 |

The generation-0 and generation-1 training panel bytes match between A and D
and match the predeclared common panels. Every trained lower checkpoint matches
the initial lower SHA-256
`353d560842da6c4270605da552a04adc8d96411512d8d3b847625cf9c502dea9`.
The resumed D generation reports `resumed: true`, 3,200 total updates and 147
prior successful outer roots available for its 128 replay slots per epoch.
The W&B runs are [A](https://wandb.ai/acrylicshrimp-personal/endfield-aic-layout-gen/runs/ffae5089721aa13fbfe5c6e5)
and [D](https://wandb.ai/acrylicshrimp-personal/endfield-aic-layout-gen/runs/caa43279e6adcb3edfe3f1c5).

D solved seven more diagnostic cases at generation 1, gaining 18 cases that A
failed and losing 11 that A solved. This is a **functional pass**, not evidence
that either factor improves stability: D changes both factors, and one short
comparison still shows substantial case churn. The eight-generation four-arm
screen is the next evidence gate. It will not use this smoke result to tune
hyperparameters or select a checkpoint.

The compact
[`smoke evidence`](2026-09-23-hierarchical-policy-stability-smoke-evidence.json)
contains exact config, input, collection, report, model and binary hashes,
case-level diagnostic outcomes, and both W&B acknowledgements. The source
identity is commit `31555f5`; the server release binary SHA-256 is recorded in
that evidence file. Server work is under
`~/Srv/aic-policy-stability-20260923` and was copied from commit parent
`4a58eba` plus the source files in `31555f5`; the checked server source-file
hashes match the committed files.

## Reproduction

From `~/Srv/aic-policy-stability-20260923`, set the LibTorch environment from
[`training-server.md`](../maintenance/training-server.md). Prepare the common
panels with `learning prepare-hierarchical-panel` using the tracked sampling
config, seeds `20260930` through `20260937`, namespace
`hierarchical-policy-stability-v1-gNNNNNN`, and the bounds in the curriculum
config. Prepare 256 train-source diagnostic candidates with seed `20261000`
and namespace `hierarchical-policy-stability-diagnostic-v1`, then run
`select-diagnostic.py` from the benchmark directory to select 64 disjoint
cases. The exact selected panel hash is in the evidence file.

The A command is:

```bash
target/release/aic-cli learning train-hierarchical-curriculum \
  --config docs/benchmarks/hierarchical-policy-stability/smoke-curriculum-a.json \
  --search-config docs/benchmarks/hierarchical-policy-stability/search.json \
  --generation-config docs/benchmarks/hierarchical-policy-stability/generation-a.json \
  --sampling docs/benchmarks/direct-constructive-full-graphs/sampling.json \
  --heldout-panel artifacts/policy-stability-inputs/diagnostic-panel.json \
  --initial-lower-output artifacts/policy-stability-inputs/initial-lower \
  --initial-outer-output artifacts/policy-stability-inputs/initial-outer \
  --catalog-root data/game/normalized \
  --output artifacts/policy-stability-smoke-a --device cuda:0
```

Replace `a` with `d` in the config and output names for the D command. The
generation-87 `training/lower` and `training/outer` checkpoints from the
stopped canonical run supply the weights. The outer initial `config.json` is
the tracked eager `outer-initial.json`, while the other initial files are exact
copies. Re-run an interrupted arm with the same arguments plus `--resume`.

## Cleanup

The two completed smoke outputs have no continuation requirement. After
checking the compact evidence and both W&B acknowledgements, their raw search
reports, training panels, optimizer checkpoints, temporary logs and the
regenerable 256-candidate diagnostic source were removed. The
[`cleanup manifest`](2026-09-23-hierarchical-policy-stability-smoke-cleanup.json)
lists the exact paths and records 464,501,739 file bytes removed; filesystem
free space rose from 176,917,938,176 to 177,382,887,424 bytes. The selected
diagnostic panel and common training panels remain because the screen uses
them.
