# Quality-replay execution readiness

Status: passed with the independently reviewed time-aware verification contract;
scientific arms have not started. The prior experiment's findings remain withdrawn.

## New parent and bounded execution

The main duration run was gracefully paused at update 1908, after its complete
32-stream wave. Its accumulated training time is 26,260.048035149004 seconds.
Its duration, source and learner settings were not changed. It remains paused.

New isolated source and artifacts are on the server under
`/home/ashrimp/Srv/aic-quality-study-20260914`. Readiness used the actual main
checkpoint, fresh native-quality metadata, the production compact source index,
private input banks and normal CUDA/persistent-worker execution. Existing panel
evaluations in this diagnostic are not scientific selection evidence.

One score-8 branch executed updates 1909–1912, retaining its first live checkpoint
and source dependencies. A separate branch restored that checkpoint and executed
one update to 1910. Both commands completed. Exactly five new normal updates were
executed. Artifact creation/failure timestamps bound the original diagnostic to
approximately 172 seconds, below its 600-second ceiling.

## Why the original comparison failed

The post-1910 model comparison failed. The two runs produced rewards
1.4621655845312966 and 1.3436108375755313. This failure is retained in
`artifacts/readiness/failure.json`; it is not a passing exact-trajectory result.

Direct comparison of the saved live checkpoint and the restored initial checkpoint
found every field identical except the explicitly rebound protocol hash: model,
both Adam states, both RNG states, complete buffer, streams, actual inputs and
native checkpoint state. Both next updates completed all 32 native-valid episodes.

Retained first-step rows show identical physical observations, decisions and all
other packed input fields, while global feature index 5 differs. That field is
`elapsed_ms / elapsed_limit` in `declarative_learning_input.prepare`. For example,
stream 2 changed from approximately 0.111129 to 0.121043. The policy consumes this
feature. Subsequent recollection is therefore not an identical-input replay.

The verified correction preserves the existing time input and native allowance.
It checks exact saved-state restoration and a valid live followup, plus a separate
exact CUDA learner transition on identical recorded native inputs. It does not
assert that different live wall-clock observations must produce identical future
actions, nor add normal training updates to replace the failed comparison.

## Verification so far

- Server: 58 declarative native/learning/retention/sampling/curriculum/execution
  tests passed, with no skips.
- Initial quality tests: all eight profile kernels preserved model, Adam, RNG,
  events and forward-call counts with the observer enabled versus disabled.
- Focused checks cover gain-only weighting, quality frequency despite identical
  coverage, actual raw/corrected SIL allocation, priority-state continuation,
  declared second-replicate RNG resets and atomic input rejection.
- `cargo check --workspace` and `cargo test --workspace` passed locally.

Final closeout passed in 38.38 seconds. All 32 actual live streams restored with
identical non-time native/input state and monotonically preserved elapsed time.
The four uninterrupted updates performed 600 actual evictions. Current and live
roots retained 5,451 rows across 42 sources in the uninterrupted branch and
5,109 rows across 40 sources in the restored branch. Compact publication and the
unchanged paused parent were verified.

The identical-input CUDA proof passed all eight profiles in 5.86 seconds. It
compared warm, restored and observed-restored learner transitions, with nonempty
Adam and identical complete input payloads. No additional normal updates ran.
The independent third review returned PASS_WITH_NOTES; all implementation notes
were checked. Final receipts are `readiness.json` and `cuda-proof.json` under
`/home/ashrimp/Srv/aic-quality-learning-20260914/artifacts/`, with local copies in
ignored `artifacts/quality-study-20260914/`. Original execution hashes and new
verification hashes are separately recorded; shared execution code is unchanged.

## Reproduction

```sh
python tools/factory-edit-pilot/operations/declarative_quality_readiness.py \
  --parent /home/ashrimp/Srv/aic-declarative-efficiency-20260913/artifacts/resumed-four-hour \
  --parent-source /home/ashrimp/Srv/aic-declarative-storage-20260914 \
  --output /home/ashrimp/Srv/aic-quality-study-20260914/artifacts/readiness \
  --environment /home/ashrimp/Srv/aic-declarative-batch-20260913/.tracking.env
```

The command above records the original invocation, whose output retains the
failure. The corrected readiness command additionally requires `--cuda-proof`.
The existing evidence was closed without rerunning those five updates:

```sh
python tools/factory-edit-pilot/operations/declarative_quality_closeout.py \
  --output /home/ashrimp/Srv/aic-quality-study-20260914/artifacts/readiness \
  --execution-source /home/ashrimp/Srv/aic-quality-study-20260914 \
  --cuda-proof /home/ashrimp/Srv/aic-quality-learning-20260914/artifacts/cuda-proof.json \
  --receipt /home/ashrimp/Srv/aic-quality-learning-20260914/artifacts/readiness.json
```

These outputs already exist. Do not overwrite their bound receipts.
