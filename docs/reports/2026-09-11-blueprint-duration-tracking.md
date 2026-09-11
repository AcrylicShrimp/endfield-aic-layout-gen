# W&B attachment to the running duration experiment

The twelve-hour runner had started training but omitted W&B integration.
An independent observer now exports its existing authenticated update and
periodic evaluation records to the established online project. Training continued
in the original PID 12177 throughout attachment and observer resume verification.

Run: [blueprint success, 64 actions, seed 17](https://wandb.ai/acrylicshrimp-personal/endfield-aic-layout-gen/runs/e27974e4e6061cd9bf19099e).
At 18:06:56 KST on September 11, W&B reported `running`; training had recorded
296,960 actions in 1,497.78 seconds. These are progress counts, not final quality.
The original 17:41:52 KST start and twelve-hour budget remain in force.

## Behavior and verification

- The observer backfills scalar training rewards, PPO diagnostics, positive
  replay labels, buffer counts and evaluation results split by original/overlap
  start. The history axis is real training actions, including the initial
  evaluation. W&B history timestamps indicate export time, not training time.
- A running frontier is withheld for one update so a just-published periodic
  evaluation cannot reorder the exported prefix. Terminal state flushes all
  remaining records. SHA-256 checks authenticate each source JSON.
- Actual observer SIGTERM, acknowledged export through step 1122, and launchd
  restart resumed the same run. All 1,154 subsequently downloaded history rows
  had contiguous ordinals and matched every exported scalar against local
  evidence (relative tolerance 1e-5, absolute tolerance 1e-7).
- Five focused tests cover late evaluation publication, final flush, resume
  cursors, hash rejection, numeric metrics, trainer liveness, successful finish,
  failed attachment and credential redaction. All pass.
- Full native-enabled Python suite: 398 tests, 7 skipped, all others passed.
  `cargo check --workspace` and Rust workspace tests passed (563 passed, 5 ignored).
  An initial Python invocation omitted the native-library setting, skipped 74
  tests and failed an existing process-completion-order assertion under concurrent
  load. The complete native-enabled rerun passed, including that assertion;
  no collector implementation or test was changed.
- Frozen training code, protocol, model/optimizer/RNG state and native sessions
  were not modified. No dependency or Rust code changed. The constructor and
  exact research solver are unaffected. Worktree/index artifact checks passed.

## Operation

The [observer contract](../designs/2026-09-11-blueprint-duration-tracking.md) defines
inputs, stable ordering, failure handling and upload scope. Invoke it with:

```sh
python tools/factory-edit-pilot/blueprint_duration_tracking.py --output RUN --env-file /path/to/tracking.env
```

The actual observer runs frozen copies under the existing run's
`wandb-observer-source/` directory. Its separate launchd label is
`gui/501/org.aic.blueprint-success-64-12h-wandb-20260911`; `wandb-observer.json`
contains its URL and heartbeat. It remains alive until the trainer stops, then
finishes W&B with explicit training status. This marks training termination,
not completion of the trainer's subsequent independent tail audit.

An observer failure records a diagnostic and leaves training running. Resume
telemetry with `launchctl kickstart
 gui/501/org.aic.blueprint-success-64-12h-wandb-20260911` after correcting the
reported failure. There is no automatic restart or external notification.
Stopping the observer cannot stop training. Credentials are read from the
existing `.env` and are not copied into manifests, artifacts or W&B configuration.

The [provenance](2026-09-11-blueprint-duration-tracking-provenance.json) binds
immutable online verification, resume observation, frozen observer sources and
verification logs. Mutable trainer and observer status files are not hash-pinned.

## Workspace panels

The user's existing workspace displayed historical keys such as
`evaluation/failed_actions` and `evaluation/all_admitted/...`. The duration
observer exports different, explicitly scoped keys, so those historical panels
had no matching series for this run. Upload verification alone did not ensure
that the user's selected panels would display the new metrics.

Created [Blueprint 64-action training](https://wandb.ai/acrylicshrimp-personal/endfield-aic-layout-gen?nw=z1oi3o9iuzd)
with ten panels for actual reward, successful commits, buffer size, positive
replay labels, imitation/value loss, KL, actor/critic epochs and validated
original/overlap evaluation outcomes. Evaluation uses points so even the initial
single observation is visible. Both sections are open and the run filter selects
this protocol. The initial evaluation remains the only point until the first
approximately 30-minute evaluation; training metrics update continuously.

Used the official [W&B Workspace API](https://docs.wandb.ai/models/ref/wandb_workspaces/workspaces)
in a separate disposable admin virtual environment under `target/`. This does not
add dependencies to the active learner or observer. Every panel metric exists in
remote history, and all ten saved panel titles, x axes and y keys were read back
and checked. The API returns metric objects rather than strings on readback; the
verification normalizes their names. Existing personal workspace settings remain
available. No browser rendering inspection was performed.

`blueprint_duration_workspace.py --run RUN --output NEW_VERIFICATION_JSON` creates
this view; `--verify-url URL` checks the saved view without creating another.
Use an environment with `wandb-workspaces` installed and the existing W&B settings.
The script and immutable workspace verification are retained for reproduction.
