# Restored lower training with horizon-12 search: primary result

Status: the predeclared primary gate passes. The reserved held-out evaluation
is required before adopting this setup. This result does not show monotonic
policy learning.

## Question and method

Does the horizon-12 search advantage survive three generations of fresh
on-policy training when both lower and outer models are updated? Three
independent streams use nine disjoint 32-case training panels and the same
separate 64-case diagnostic panel. Each stream begins from the generation-87
weights. Horizon six and horizon 12 collect their own trajectories and train
both models for 100 epochs with the original policy target. The mixed arm
evaluates the horizon-six-trained weights under horizon-12 search without
another training trajectory. Other controls are fixed: 128 simulations,
16 root candidates, eight anchors, and root seed 20260922. The selected
promotion candidate was fixed to horizon-12 training/search before the
outcomes were seen.

The [protocol](2026-09-26-hierarchical-lower-restored-protocol.json) fixes the
source, input hashes, disjoint panels, candidate, and numeric gate. The
[progress](2026-09-26-hierarchical-lower-restored-progress.json) retains all
27 diagnostic masks and collection, checkpoint, training-report, and
evaluation hashes. Source runner commit `ad28ad04`; the source was not changed
during this experiment. [W&B tracking](2026-09-26-hierarchical-lower-restored-wandb-observer.json)
finished with all 27 records remotely acknowledged at [the run](https://wandb.ai/acrylicshrimp-personal/endfield-aic-layout-gen/runs/7340b46d889448b76ae70368).

## Results

Each cell is `completed / lost starting successes` of 64 cases. The starting
horizon-six search completes 29 cases; horizon-12 search completes 41 with the
same starting weights. `Lost` refers to those 29 horizon-six starting
successes.

| Stream | Generation | H6 train/search | H6 train, H12 search | H12 train/search |
| --- | ---: | ---: | ---: | ---: |
| s1 | 1 | 22 / 17 | 40 / 7 | 35 / 10 |
| s1 | 2 | 29 / 15 | 44 / 9 | 39 / 11 |
| s1 | 3 | 30 / 12 | 47 / 4 | 34 / 12 |
| s2 | 1 | 25 / 17 | 37 / 9 | 36 / 12 |
| s2 | 2 | 22 / 19 | 37 / 10 | 48 / 3 |
| s2 | 3 | 29 / 13 | 38 / 7 | 49 / 4 |
| s3 | 1 | 26 / 16 | 46 / 5 | 40 / 8 |
| s3 | 2 | 23 / 16 | 35 / 11 | 38 / 12 |
| s3 | 3 | 30 / 13 | 45 / 8 | 36 / 9 |

At generation three, the prespecified H12/H12 versus H6/H6 completion gains
are `+4`, `+20`, and `+6` cases, mean `+10/64`. The differences in lost
starting successes are `0`, `+9`, and `+4` fewer, mean `4.33` fewer. Two
streams have positive retention, and all six generation-two/three completion
contrasts are positive. All five [primary checks](2026-09-26-hierarchical-lower-restored-primary-summary.json)
pass. The reserved 128-case held-out panel, evaluated under three independent
root seeds, is the predeclared final gate.

The first-root [target audit](2026-09-26-hierarchical-lower-restored-target-quality-summary.json)
finds mean target mass on unvisited actions of `99.47%` for H6 and `99.07%`
for H12 across 288 roots per arm. H12 improves search outcomes but still
provides an extremely diffuse first-root policy target. Under the same H12
search, H12-trained weights complete `13` fewer, `11` more, and `9` fewer
cases than H6-trained weights across the three final streams. Final H12/H12
completions average `39.67/64`, slightly below the untrained H12-search start
at `41/64`. These comparisons do not establish a consistent gain from
the learned weights. They support the narrower conclusion that deeper search
remains useful while lower-model training is enabled.

## Reproduction and continuation

The server workspace is `~/Srv/aic-target-causal-20260925`; the result root
is `artifacts/recovery-lower-restored-v2`. Use the recorded LibTorch runtime
from `docs/maintenance/training-server.md`, the source commit and hashed
inputs in the protocol, and these commands:

```sh
python3 tools/factory-edit-pilot/hierarchical_lower_restored.py run \
  --output artifacts/recovery-lower-restored-v2
python3 tools/factory-edit-pilot/hierarchical_lower_restored.py summarize-primary \
  --output artifacts/recovery-lower-restored-v2
python3 tools/factory-edit-pilot/hierarchical_lower_restored_target_audit.py \
  --output artifacts/recovery-lower-restored-v2
```

The source panels and final checkpoints remain available for the reserved
held-out continuation. The [cleanup manifest](2026-09-26-hierarchical-lower-restored-cleanup.json)
records which replayable intermediate outputs were removed after the compact
evidence was retained.
