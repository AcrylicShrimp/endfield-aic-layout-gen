# Online physical outcome encoder pretraining setup

## Ready configuration

The online encoder pretraining path is ready for a 12-hour CUDA run. It starts
the established width-40, two-round graph encoder from seed `950000`; it does
not load the earlier physical-outcome dataset or its trained weights. Every
training update generates 16 new roots, collects 32 native physical-editor
actions per root through one persistent 16-worker pool, trains four-root
minibatches once, and releases all rows without writing them to disk.
Each root owns a 1,024-seed lane and may try up to 16 seeds, so an exhausted
procedural seed cannot permanently block resume or overlap another root.

The registered configuration is
[`tools/native-training/physical-outcome-online-12h.json`](../../tools/native-training/physical-outcome-online-12h.json).
It alternates 8-block and 16-block roots. Training and development use disjoint
generation and action seed namespaces. Every 100 updates, the runner generates
16 separate development roots, evaluates them once, and discards them.

Checkpoints are committed every 10 updates. Only the latest checkpoint is kept;
it contains the encoder, disposable outcome heads and Adam state. Resume starts
at the next unused root ordinal. The final `encoder.safetensors` contains only
the shared encoder allowlist. A `STOP` file requests a checkpointed stop after
the current update.

## W&B

[`physical_outcome_online_tracking.py`](../../tools/factory-edit-pilot/physical_outcome_online_tracking.py)
is an independent observer. It reads committed `metrics.jsonl` records and the
durable state file, then resumes a deterministic W&B run for the output name.
Training never waits for W&B. The observer reports update, elapsed time, roots,
rows, train loss, gradient norm, throughput and the latest development loss.

The online connection was verified with a real two-stage resume run:
[W&B readiness run](https://wandb.ai/acrylicshrimp-personal/endfield-aic-layout-gen/runs/997de9a8180e3fa5c7f0697f).
W&B acknowledged both committed records after the second process resumed the
same run.

## Integrated readiness evidence

On the NVIDIA GeForce RTX 5090 host, the first CUDA update generated 16 fresh
roots and 512 fresh rows, trained the model, and published a checkpoint in
1.29 seconds. Peak process-tree RSS was 1.75 GB. Resuming that checkpoint for a
second update produced 32 total roots and 1,024 total rows; the encoder hash
changed from `b6ab5ea0f38ef4188d83ae35ab99f26e7fea8813426511282b4f338d55f03bea`
to `01a057fc107883d69445f4b48f3a3f24f5bb1f367c1beba8689ccefeb45bb9e0`.

A separate one-update run forced the development cadence to one. It generated
and discarded 16 training plus 16 development roots, recorded 512 rows for each
namespace, and emitted development loss `2.202393`. This checks the complete
live generation, training, held-out evaluation and publication route without
reusing stored examples.

## Launch commands

The established host uses the following trainer command from the repository
root. Use a fresh output directory for the 12-hour run.

```sh
env \
  LIBTORCH=/home/ashrimp/Srv/aic-rust-learning-20260915/.venv/lib/python3.12/site-packages/torch \
  LIBTORCH_CXX11_ABI=1 \
  LD_LIBRARY_PATH=/home/ashrimp/Srv/aic-rust-learning-20260915/.venv/lib/python3.12/site-packages/torch/lib \
  RUST_MIN_STACK=16777216 \
  CUBLAS_WORKSPACE_CONFIG=:4096:8 \
  target/release/aic-cli learning pretrain-outcomes-online \
  --config tools/native-training/physical-outcome-online-12h.json \
  --catalog-root data/game/normalized \
  --output artifacts/physical-outcome-online-12h \
  --device cuda:0
```

Start the observer after `state.json` appears:

```sh
/home/ashrimp/Srv/aic-masked-long-opt-20260914/tools/factory-edit-pilot/.venv/bin/python \
  tools/factory-edit-pilot/physical_outcome_online_tracking.py \
  --output artifacts/physical-outcome-online-12h \
  --env-file .env
```

After interruption, run the trainer command with `--resume`; the observer uses
W&B `resume="allow"` automatically.
