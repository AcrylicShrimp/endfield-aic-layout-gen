# Fresh padded PPO twelve-hour launch

The new experiment started at **2026-09-11 01:10:31 KST**, with a fresh
43,200-second allowance ending nominally at **2026-09-11 13:10:31 KST**, followed
by bounded finalization. This report verifies launch; twelve-hour training is
still running.

The run uses implementation `d20b416`: shared-memory collector delivery,
overlapped CPU preparation and the masked padded CNN/spatial indexing learner.
Its [readiness report](2026-09-11-ppo-batched-indexing.md) records 79 passing
native/CUDA PPO tests, factory and lineage validation, and exact reconstruction
of a complete PPO update within the new runtime. Historical and new learner
updates are not bit-exact; no old trained state is imported into this run.

## Fresh initialization and schedule

The actor, critic and CPU/CUDA/shuffle RNG states match a newly instantiated
seed-17 learner exactly. Both Adam state dictionaries are empty, and the initial
checkpoint is batch zero. The protocol has no continuation field; the new run
starts with an empty curriculum bank. Seed 17, frozen factory inputs and the
evaluation cohort are retained for comparability. Reusing initial factory
geometry does not import model training or predecessor curriculum state.

The existing `ppo-cnn2-curriculum-twelve-hour.json` configuration selects:

- Sixteen CPU collectors and one CUDA FP32 learner.
- Sixty-four factory attempts per training batch, 32 actions per episode.
- Sixty-four selections per PPO minibatch, four epochs and the existing KL stop.
- An 80/10/10 fresh/one-pass/two-pass best-state curriculum with bootstrap
  substitutions while the new donor pools form.
- The two-layer width-32 CNN, two graph layers, 50,000 routing-work budget and
  0.001 failure penalty.
- The fixed 32-factory evaluation every four batches; frozen fresh-input slots
  for the first sixteen training batches, then ordinary configured generation.
- The existing 32 GiB process-tree RSS limit, 16 GiB disk reserve and selective
  full-trace retention. The 8,192-batch schedule supplies headroom; the twelve-hour
  duration and resource limits determine actual completion.

The previous experiment remains stopped at 32 batches / 64,608 actions, with
its original runtime, checkpoints and records preserved.

## Verified live learning

The first four batches committed 8,000 admitted actions and hash-verified
checkpoints. All 32 inherited inputs passed the consecutive-parent, best-geometry,
request-obligation and current-policy checks. The four batches contain 224 fresh,
19 one-pass and 13 two-pass attempts. Start plans are deterministic, and
evaluation never inherits the training curriculum.

W&B's remote configuration matches the frozen protocol, and all 565 exported
scalar values for training batches 1-4 and evaluations 0/4 match local committed
records. Fixed-cohort mean area reduction is 47.1164% at initialization and
50.9153% after four batches. This is launch verification, not a long-run quality
conclusion. Six batches were already committed when the cloud check completed.

The [provenance manifest](2026-09-11-padded-ppo-twelve-hour-launch-provenance.json)
binds the source archive, configuration, native library, protocol, fresh initial
checkpoint and live checks. Documentation/artifact-policy and Git whitespace
checks passed; this launch changes no runtime code or training configuration.

## Runtime and controls

Frozen runtime:
`/home/ashrimp/Srv/aic-ppo-padded-curriculum-12h-20260911`.

Run: `artifacts/factory-edit-pilot/padded-curriculum-twelve-hour`.

User service: `aic-ppo-padded-curriculum-12h-20260911.service`, `Restart=no`.
User lingering is enabled, so SSH and conversation closure do not stop training.
The normal harness duration stop preserves the last committed checkpoint and
partial non-replayable work. Do not change imported runtime sources while it runs.

[Live W&B run](https://wandb.ai/acrylicshrimp-personal/endfield-aic-layout-gen/runs/86bdb8ba1b448abf0f4fc4cc).

From the frozen runtime:

```sh
PY=tools/factory-edit-pilot/.venv/bin/python
RUN=artifacts/factory-edit-pilot/padded-curriculum-twelve-hour
$PY tools/factory-edit-pilot/ppo_harness.py status --run "$RUN"
$PY tools/factory-edit-pilot/ppo_harness.py stop --run "$RUN"
```

Launch used the public `prepare` command with the tracked configuration and a
new output, then `run --duration-seconds 43200` under `systemd-run --user`.
W&B credentials are read from the existing external environment file; they are
not stored in the source archive or provenance.
