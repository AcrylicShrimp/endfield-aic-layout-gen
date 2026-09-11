# Launch of twelve-hour, 64-action blueprint training

Status: launch verified; training is running. This is a launch report, not a
completed twelve-hour quality result.

The user requested twelve hours and explicitly confirmed that 64 is the episode
length, replacing 256. Training started on **2026-09-11 at 17:41:52 KST**
(08:41:52 UTC). The active training budget ends around **2026-09-12 at 05:41:52
KST**, followed by at most three minutes to finish open episodes and final
evaluation. A separately reported audit of the last two updates follows.

## Executed configuration

- One fresh seed, 17; no old model or successful experience is imported.
- 64 policy actions per true episode; eight streams and 32 actions per chunk.
- Original/overlap starts, equally represented; positive-return-advantage
  self-imitation coefficient .05 and the existing PPO settings.
- Evaluation at initialization, approximately every 30 minutes, and termination:
  three samples per start, at most 64 policy actions and two native seconds.
- 43,200 active wall-clock seconds, including I/O and evaluation; resume consumes
  the remaining duration. The two-million-update ceiling is an additional guard.
- Lossless gzip-six evidence, authenticated original chunks, immutable success
  input caching, incremental optimizer-checkpoint retention, 96-GiB output cap
  and a 20-GiB free-space reserve.

The [registered contract](../designs/2026-09-11-blueprint-success-64-12h.md) and
`tools/factory-edit-pilot/blueprint-success-64-12h.json` define the run. The code
was committed as `60c8cab` before execution and checked out into the detached
worktree `artifacts/factory-edit-pilot/blueprint-64-12h-source-20260911/`. Runtime
source checks use that frozen tree, allowing subsequent work in the main tree.
All game catalogs and the initial fixture remain externally loaded and hash-bound.

## Observed launch and verification

At 17:43:58 KST, the process had completed 125 updates and 32,000 policy actions
in 126.64 active seconds. It had archived two positive-area commits and admitted
two sequences; the first admission was update eight. Actual optimizer changes,
durable checkpoints and positive-weight reuse are recorded. These early counts
are startup evidence and do not establish long-run effectiveness.

- Full Python suite: 393 tests run, 7 skipped. Rust: 563 passed, 5 ignored;
  `cargo check --workspace` also passed. No Rust source or dependency changed.
- Actual CLI pause after the first 32-action chunk, then resume: all six updates
  and 1,536 actions replay exactly, including learner, Adam, RNG and buffer state.
  Native audit checks 48 restorations, 24 continuing boundaries, 24 true terminals,
  and 24 completed 64-action episodes.
- A real launchd deadline trial finished its open episode and passed its final
  two-update audit. A separate Standard-process launchd trial honored SIGTERM at
  a true terminal and passed the same audit.
- Unit checks cover deadline reasons, single-writer locking, cache equality with
  the original resolver, cache reconstruction, checkpoint retention and lossless
  serialization. The cache changes no sampling, weighting or buffer semantics.
- Worktree/index artifact checks pass. Native topology, endpoint, rate and full
  validation rules remain unchanged; no production-constructor or exact-solver
  behavior changes.

The [launch provenance](2026-09-11-blueprint-success-64-12h-provenance.json) binds
immutable startup and readiness evidence. It deliberately does not hash the
changing live state or completion files. Raw training, checkpoints and logs remain
ignored under `artifacts/`. The two completed readiness launchd registrations were
removed after retaining their evidence.

## Monitoring and stopping

Run directory: `artifacts/factory-edit-pilot/blueprint-success-64-12h-20260911/`.
The launchd job is `gui/501/org.aic.blueprint-success-64-12h-20260911`, initially
PID 12177. Its process uses Standard classification and holds an idle-sleep
prevention assertion for its lifetime. The job is not configured to restart
automatically or to become a recurring login task.

```sh
cat artifacts/factory-edit-pilot/blueprint-success-64-12h-20260911/state.json
tail -n 20 artifacts/factory-edit-pilot/blueprint-success-64-12h-20260911/job.log
launchctl kill SIGTERM gui/501/org.aic.blueprint-success-64-12h-20260911
```

SIGTERM requests an episode-boundary pause with saved state. Resume a paused job
with `launchctl kickstart gui/501/org.aic.blueprint-success-64-12h-20260911`.
An already completed run requires a new output directory and budget.

`state.json` reports live duration, actions, update, successful commits, buffer
entries, checkpoint and evaluation references. `completion.json` records the
eventual stop reason and tail-audit reference. `audit.json` states its precise
audited interval; the automatic last-two-update audit is not a full-run audit.
Use the frozen `audit_blueprint_duration.py --run RUN --all` for an explicit full
replay afterward. Preserve the frozen source worktree while this job is active.
