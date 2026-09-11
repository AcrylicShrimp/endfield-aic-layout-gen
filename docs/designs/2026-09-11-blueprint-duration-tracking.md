# Blueprint duration W&B observer

The duration runner omitted W&B integration. Attach a separate CLI observer to
an existing run without restarting training or editing its frozen protocol.

- Input: `--output` duration run directory; `--env-file` containing the existing
  W&B destination and credential; optional polling interval.
- Output: one online W&B run, with a stable ID derived from the protocol hash,
  authenticated update/evaluation metrics, and local `wandb-observer.json` status.
- The observer reads trainer state and hash-bound JSON records only. It never
  loads or changes the model, optimizer, RNG, native sessions, or protocol.
- Order history by update, with evaluation after training at the same update.
  While running, withhold the latest update until the next update is durable;
  this prevents an evaluation published after a checkpoint from changing an
  already exported event prefix. Stopped state releases the final update.
- Resume from acknowledged W&B history and the SDK resume cursor, never merely
  from a locally queued cursor. Reject a remote cursor ahead of source evidence.
- History uses real training action/update axes. Backfill timestamps are export
  times, not original training times. Original start and current elapsed time
  appear in summary; do not invent missing per-update elapsed timestamps.
- Summaries distinguish trainer status, observer attachment, and completion.
  The observer stays alive while training runs and finishes when training stops.
  Missing/dead trainer PID is an observer failure, not successful training.
- Use a separate exclusive observer lock. Export/network failure cannot stop
  training. Store a structured diagnostic, redact credentials, and allow an
  explicit observer restart to resume the same W&B run.
- Do not upload checkpoints, native evidence, environment files, code, machine
  metadata or console logs. Existing scalar metrics and public run config suffice.

Verify prefix stability around evaluation publication, resume backfill, metric
values, hash rejection, stop/death classification, and a real online observer.
