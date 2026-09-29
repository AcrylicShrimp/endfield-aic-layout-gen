# Plan .01 local one-shot caller calibration closeout

## Question and method

On the same physically valid synthetic upper-grid source, compare the existing one-shot `Session::route_candidates` caller before and after the current native replay work. The source contains 256 placed facilities, 254 committed obligations, both belt and pipe transport, and a pipe candidate crossing an installed belt route. The fixture builds all placements and prior routes with accepted ordinary native commands, then times only the final candidate caller in an ignored release test. It asserts one candidate and one bridge crossing and prints its cells, six commands, crossing, request peak reservation and caller elapsed time. `/usr/bin/time -l` measures caller-only test-process RSS. The current fixture returns after this output when `AIC_CALIBRATION_CALLER_ONLY=1`; the earlier code uses the same temporary caller section. No server was used.

## Code identity and reproduction

- Prior source: committed `b76c4bea`, plus the temporary `baseline-test.patch` retained under ignored `artifacts/plan01-caller-2026-09-27/`. SHA-256: `a868aa99624da816cb729d94ca0360e19c7b50e460a5efd54f96b7180fc22d86`.
- Current source: committed base `b76c4bea`, plus the uncommitted tracked patch `current-tracked.patch` (SHA-256 `6fba22fd5090e2ad57ee7df7922f0ff9bf8e122dcbbde8a115ed329482f33fbb`) and untracked source archive `current-untracked.tar` (SHA-256 `2115432a77bfa802fc28d3247e4ca6905c5db7d8175279a9fc2e588526e6670a`) retained in the same ignored artifact directory. The archive member list is `current-untracked.txt`. These snapshots represent the measurement source, including later test-only ordinary-versus-admitted and repeated-replay calibration branches; they are local retained evidence until the source is committed, not a backup.
- Baseline reproduction: create a disposable worktree at `b76c4bea`, apply the retained baseline test patch there, build `cargo test -p aic-pilot-native --lib mixed_transport_grid_crossing_replay_calibration --release --no-run`, then run its release test binary with `--ignored --exact constructive::tests::mixed_transport_grid_crossing_replay_calibration --nocapture`. Use a separate `CARGO_TARGET_DIR` for this worktree: sharing one Cargo target directory between two source trees can leave stale dependency artifacts.
- Current reproduction: `AIC_CALIBRATION_CALLER_ONLY=1 cargo test -p aic-pilot-native --lib mixed_transport_grid_crossing_replay_calibration --release -- --ignored --nocapture`. Use `/usr/bin/time -l` around the compiled test binary for caller-only RSS.

## Result and verdict

| Caller | Candidate elapsed | Request reserved peak | Caller-only process maximum RSS |
| --- | ---: | ---: | ---: |
| Prior committed path | 2–3 ms | 43,359,488 bytes | 8,355,840 bytes |
| Current uncommitted path | 12–14 ms | 26,269,449 bytes | 8,765,440 bytes |

Both selected the same five route cells, six commands, `log-pipe-connector` crossing and its orientation. The prior reservation was conservative and exceeded actual process RSS; the current reservation shrank, while RSS was similar. The roughly 10-ms added caller time may matter when many witnesses are evaluated. The whole-caller comparison includes current uncommitted search and physical-cost changes, so it does not isolate native replay. This is a synthetic grid without terminals or a representative generated/placed factory distribution. Verdict: **continuity warning, not Plan .01 performance acceptance**. Measure how often this cost occurs in representative placed factories and reduce or justify it before completing the module; Plan .10 still owns full search-to-publication performance.

A follow-up within the current release fixture applied the selected six commands to the same source using ordinary `clone` and `Session::step` in 3 ms, then advanced `NativeReplayProgress` once in 10 ms; the resulting observations matched. This local replay comparison places roughly 7 ms of the single-candidate gap inside the admitted replay path on this source. It still does not isolate the async validator from command preparation, and the separate measurements may benefit from warmed caches. The current one-shot caller's other stages and the prior committed implementation also changed, so keep the whole-caller conclusion scoped as above.

Independent reviewer `/root/review_plan01_caller_comparison` verified the compared source construction, retained snapshot hashes and cleanup, and BLOCKED Plan .01 completion. Matching five cells, six commands and crossing is narrower than full Candidate/Report parity; the RSS values are whole fixture-process peaks. The reviewer requested one representative placed-input result/time/RSS comparison and enough stage attribution to decide whether the installed one-shot adapter's extra time is acceptable. The follow-up ordinary/admitted replay measurement above partially narrows the stage question; the placed-input gate remains open.

## Cleanup

The disposable prior-code worktree at `target/one-shot-baseline` contains only the reproducible prior commit and the retained test patch. Its test binary and build cache in ignored `target/` are regenerable. After verifying the patch and SHA-256 values, remove that worktree and record the removed path and local disk-space change below. Retain the source snapshots under `artifacts/plan01-caller-2026-09-27/` until the corresponding source is committed and this conclusion can be reconstructed without them.

Cleanup manifest, 2026-09-27 KST: verified all three snapshot hashes above, then removed `target/one-shot-baseline` with `git worktree remove --force`. The disposable worktree occupied 37,116 KiB immediately before removal. Local filesystem available blocks rose from 30,817,924 to 30,869,176 KiB (`df -k .`); the 51,252-KiB difference includes filesystem accounting and is not a per-file byte measurement. No training inputs, checkpoints, replay buffers or server artifacts were created by this calibration. The ignored snapshot directory remains intentionally retained while Plan .01 source is uncommitted.
