# Horizon-12 on-policy setup: stopped before training

Status: closed as a preparation failure. No training record was produced.
The completed initial diagnostic is valid and is reused by the corrected
runner; this attempt is not a learner result.

The experiment at source commit `3a210375` regenerated all nine 32-case
training panels and the 64-case diagnostic panel with hashes equal to the
retained horizon-six on-policy protocol. It copied the generation-87 outer
checkpoint unchanged and changed only its search contract to horizon 12.
The [protocol](2026-09-26-hierarchical-h12-initial-attempt-protocol.json)
records the exact binary, runner, helper, model, search and panel hashes.
W&B acknowledged readiness at step zero before the first diagnostic search.

The initial horizon-12 search completed 33/64, versus 26/64 for the same
checkpoint under horizon six on the same cases and IDs. The paired mask
contains 16 new successes and nine lost starting successes. Its case search
times sum to 207.24 seconds. The [compact summary](2026-09-26-hierarchical-h12-initial-attempt-summary.json)
retains the case mask, report hash and paired counts. This is a single-seed
starting-search measurement, not evidence that training retains the gain.

Immediately after the CLI wrote a complete report, the Python runner
mistakenly compared the raw search-config file SHA-256 against the report's
hash of the parsed configuration. The panel and both checkpoint hashes
matched; the configuration's compact semantic JSON hash also matched the
report. The runner stopped before appending the initial result to
`progress.json` and before collecting or training any case. The
[progress](2026-09-26-hierarchical-h12-initial-attempt-progress.json) has zero
records. The [observer](2026-09-26-hierarchical-h12-initial-attempt-wandb-observer.json)
was detached after its readiness record; no training metrics were sent.

Commit `48aae78c` fixes the validation to compare the parsed configuration
hash while still retaining the exact source-file hash in the protocol. The
corrected runner was checked against the completed report's semantic hash.
The original 233,221,709-byte diagnostic report was copied by verified
SHA-256 to `~/Srv/aic-target-causal-20260925/artifacts/recovery-horizon-initial-reuse/start-h12.json`
for one-time transfer into the corrected experiment. That temporary copy
must be removed after the corrected progress records the initial result.

To reproduce the stopped attempt under commit `3a210375`, run
`python3 tools/factory-edit-pilot/hierarchical_horizon_onpolicy.py prepare --output artifacts/recovery-horizon-onpolicy --experiment-commit 3a210375`,
start the documented W&B observer and verify step-zero acknowledgement,
then run `python3 tools/factory-edit-pilot/hierarchical_horizon_onpolicy.py run --output artifacts/recovery-horizon-onpolicy --max-records 0`
with the documented LibTorch environment. The failure occurs after the
initial CLI search returns.
The [cleanup record](2026-09-26-hierarchical-h12-initial-attempt-cleanup.json)
removes 250,743,594 bytes of stopped preparation state after the diagnostic
report's temporary continuation copy and compact closeout were verified.
The corrected runner then accepted that report as its initial diagnostic;
the [reuse cleanup](2026-09-26-hierarchical-h12-initial-reuse-cleanup.json)
records removal of the temporary second copy.
