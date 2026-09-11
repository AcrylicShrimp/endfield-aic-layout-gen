# Twelve-hour blueprint self-imitation run

Status: authorized by the user. The user explicitly selected 64 actions per
episode, replacing 256. Register this duration run before execution.

## Contract

Use one fresh seed, 17, with the current .05 positive-return-advantage imitation
loss, initially empty success buffer, original/overlap starts, eight streams,
32-action chunks and the existing PPO settings. True episode termination is now
64 policy actions. Starting fresh avoids transferring a critic and terminal
returns learned for the old 256-action horizon. No prior run experience or
scripted success enters main training.

The run lasts 43,200 seconds of active process wall time, including collection,
updates, I/O and evaluation. Resume continues the remaining budget. At the
deadline, finish the open 64-action episodes and stop before starting new ones.
Allow at most 180 seconds of training finalization grace. SIGTERM/SIGINT request the same
durable boundary stop; a hard guard or error preserves the partial chunk, current
learner/buffer and native streams. Never present a resource stop as a full
twelve-hour completion.

Evaluate at initialization, approximately every 30 minutes at a completed-episode
boundary, and at termination. Use three samples for each original/overlap start,
64 policy actions and the existing two-second native acceptance guard. Report
actual actions and elapsed time. Native validation remains mandatory for every
retained best. Evaluation never adds buffer labels.

Use an explicit `blueprint-success-duration-v1` protocol/state and CLI. The fixed
screen runner, configuration and evidence remain immutable. Keep every chunk
and update record authenticated, with original non-replayable observations.
Compress new chunks at gzip level six without changing their contents. Retain
the latest four optimizer checkpoints, initialization, the first three updates,
evaluation checkpoints and first admission plus next update; remove other
regenerable optimizer checkpoints incrementally.

Resolve immutable admitted sequence inputs once per entry and cache them in RAM.
Require exact equivalence to the original sorted sequence resolver, including
input, action index, return, admission age and references. Rebuild this cache from
authenticated chunks after resume. Sampling, weighting, deduplication and buffer
guards are unchanged. Cap generated evidence at 96 GiB and preserve at least
20 GiB of filesystem free space; crossing either guard stops with saved evidence.

State reports expose elapsed time, stop reason, update/action counts, successful
commits, buffer size, checkpoint references and periodic evaluation rows. An
independent prefix/tail audit can replay retained updates from initialization or
a retained nonempty checkpoint. Its result must state its actual audited interval;
a tail audit is not a full-run audit. The job performs that audit after training
has stopped; its completion time is recorded separately from training duration.

## Readiness and launch

Before launching the main process, verify the 32/64 chunk/terminal boundary,
cached-sequence equivalence, compressed evidence round-trip, deadline decisions,
native continuation and actual CLI resume. Replay all readiness learner/buffer
updates and check native witnesses. Run repository-required checks and commit
the executable/configuration before launching from an isolated frozen checkout.

Run under a user launchd job with idle-sleep prevention for its lifetime. Retain
PID/job label, start time, log and paths locally. Confirm actual learning and a
durable checkpoint before reporting launch. Do not claim the twelve-hour result
when reporting that training has started. Production construction and frozen
blueprint topology/endpoint/rate/validation contracts remain unchanged.
