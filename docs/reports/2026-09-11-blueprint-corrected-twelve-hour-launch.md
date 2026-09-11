# Fresh twelve-hour launch with restored evaluation

Status: running; startup verified, not a completed training result.

The corrected run started **2026-09-11 at 19:00:35 KST**. Its twelve-hour
wall-clock training budget ends around **2026-09-12 at 07:00:35 KST**, followed
by at most three minutes for terminal-boundary finalization and a separately
reported last-two-update audit. Evaluation and I/O count toward the twelve hours.

The frozen source commit is `c6ed2cd`, following native admission fix `d3d4f10`.
Its detached checkout is
`artifacts/factory-edit-pilot/blueprint-64-12h-corrected-source-20260911/`.
The [correction report](2026-09-11-blueprint-evaluation-correction.md) records
restored inputs, metric definitions and verification.

## Observed startup

- Fresh seed-17 model, zero entries in both Adam states, empty success buffer,
  and no imported checkpoint. Actual iteration-zero evaluation is exported
  before the first learner update.
- Episodes remain 64 actions, eight streams, 32-action chunks and positive-return
  replay coefficient .05. Four batches are 1,024 training actions.
- Every evaluation uses the original 32 frozen factories and original sampling
  seeds, up to 64 actions and two native seconds per factory.
- W&B server history confirms evaluation at batches zero and four with all
  32 cases admitted. Actual area reductions are 0.8165631064% and 0.3986135990%;
  paired changes are zero and -0.4179495074 percentage points. These are startup
  observations, not evidence of a positive training trend.
- At 19:01:24 KST the durable training state had reached 16 batches / 4,096
  actions. Tracking runs automatically as a supervised child of the trainer.
- The existing dedicated W&B view now selects the fresh protocol and contains
  twelve panels using canonical evaluation/PPO keys. Server metric values,
  saved panel axes and the saved run filter were read back. Missing weighted
  replay loss is left absent until positive replay labels have actually been used.

[Live run](https://wandb.ai/acrylicshrimp-personal/endfield-aic-layout-gen/runs/11537d338118ad65d0663c3b)
and [dedicated charts](https://wandb.ai/acrylicshrimp-personal/endfield-aic-layout-gen?nw=z1oi3o9iuzd).
Personal workspace definitions are unchanged; selecting this run supplies their
existing canonical keys. No browser screenshot verification is claimed.

## Reproduction and control

```sh
tools/factory-edit-pilot/.venv/bin/python \
  artifacts/factory-edit-pilot/blueprint-64-12h-corrected-source-20260911/tools/factory-edit-pilot/blueprint_duration.py prepare \
  --config artifacts/factory-edit-pilot/blueprint-64-12h-corrected-source-20260911/tools/factory-edit-pilot/blueprint-success-64-12h.json \
  --native-library target/release/libaic_pilot_native.dylib \
  --evaluation-cohort artifacts/factory-edit-pilot/blueprint-evaluation-cohort-20260911/cohort.json \
  --env-file .env --output NEW_OUTPUT_DIRECTORY
```

The running output is
`artifacts/factory-edit-pilot/blueprint-success-64-12h-corrected-20260911/`.
Launchd label `gui/501/org.aic.blueprint-success-64-12h-corrected-20260911`
uses Standard process classification, no automatic restart, and idle-sleep
prevention for the trainer lifetime. Initial trainer PID: 30768.

```sh
tail -n 20 artifacts/factory-edit-pilot/blueprint-success-64-12h-corrected-20260911/job.log
launchctl print gui/501/org.aic.blueprint-success-64-12h-corrected-20260911
launchctl kill SIGTERM gui/501/org.aic.blueprint-success-64-12h-corrected-20260911
```

SIGTERM requests a terminal-boundary pause, persisted checkpoint and tail audit.
The [provenance manifest](2026-09-11-blueprint-corrected-launch-provenance.json)
binds immutable startup snapshots, authentic cohort inputs and final readiness
proofs. Live state, completion and observer files are intentionally not hashed as
immutable evidence. Preserve the retained source and input directories while the
run is active. The earlier stopped run remains preserved separately.
