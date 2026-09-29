# Plan .11 local repair-scope preparation measurement

## Question and method

The original Plan .11 proposal retained every scan and pricing loop across preparation pages. Before adding that state machine, measure whether the existing one-shot work is an actual first-turn-scale operation and identify where a retry can discard already charged work. This is a bounded local release-mode calibration, not an admission proof for the final 240–256-facility distribution.

The ignored `plan11_placed_repair_scope_calibration` native test replays the four retained Plan .01 panels through ordinary commands, then times `PreparedRepair::new` (which includes the state hash) and `PreparedRepair::conflict_order` separately. It selects the panel's next incomplete connection and permits one removal from every committed route. The ignored `mixed_transport_grid_crossing_replay_calibration` test has an `AIC_CALIBRATION_REPAIR_SCOPE_ONLY=1` branch for a valid synthetic 256-facility, 253-committed-route source with 8,067 route cells. Each case ran in five separate release test processes on the local machine. Timers exclude panel replay and source construction but include the scope hash, conflict pricing, allocation and allowance operations. Reservations were released to zero after each run.

| Case | Facilities | Committed routes | Route cells scanned | Max scope + hash µs | Max conflict order µs | Peak request reservation bytes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Placed branching / 38 | 30 | 38 | 1,986 | 590 | 375 | 222,784 |
| Placed compact / 90 | 100 | 90 | 6,018 | 492 | 680 | 636,352 |
| Placed balanced / 90 | 100 | 90 | 7,948 | 593 | 709 | 821,888 |
| Placed spacious / 90 | 100 | 90 | 11,517 | 743 | 936 | 1,163,616 |
| Synthetic mixed grid | 256 | 253 | 8,067 | 1,250 | 779 | 914,464 |

The `scope` work includes the one-time exact-state SHA-256. The synthetic case has many facilities but repeated compact topology; it does not model the varied physical area, route lengths and conflicts intended for training. The placed cases have varied area and route length but stop at 100 facilities. These five timings do not establish a sound worst-case bound or justify a new size limit. They do show that a separate per-visit repair preparation cursor is not yet supported by measured need in these cases.

## Observed retry boundary

`lower_pager::Task::prepare_scope` currently sets `scope_prepared` before `RepairPager::new`; a stop drops that pager and its prepared scope. Scope creation reserves its main buffer before the allowed-route scan, but the state hash asks for an additional workspace reservation after the scan. An aggregate memory refusal there can discard scanned IDs and their charged work. Conflict ordering separately reserves memory before its pricing loops; an aggregate refusal there drops the completed scope and causes the next attempt to repeat its state hash. Before first-turn quotas are snapped, there is no temporary page window; work-limit and deadline stops are hard request stops, while aggregate memory refusal may be temporary. This distinction supports retaining a prepared stage across aggregate retry even if measured indivisible computation remains direct. The retained stage must not consume the other task's first-turn quota and must not silently treat a hard stop as empty scope.

## Reproduction and provenance

Base Git commit: `a0c89fd634cba72e9bb41bcf05e4a3bbbdc70f74`. The uncommitted native source and Cargo files used by these tests are retained at ignored `target/plan11-scope-calibration/current-native-source.tar.gz`, SHA-256 `8df3d12b494d3f0b2c45c9fb19c447a3d0976be0d33f713e2e145b53bd6a3d58`. Build with `cargo test -p aic-pilot-native --lib plan11_placed_repair_scope_calibration --release --no-run`, then run its `aic_pilot_native-*` release test binary with `AIC_PLAN11_SCOPE_PANEL=artifacts/plan01-placed-input-20260927/panel-NAME.json --ignored --exact constructive::tests::plan11_placed_repair_scope_calibration --nocapture`. For the synthetic case, use `AIC_CALIBRATION_REPAIR_SCOPE_ONLY=1` and `--exact constructive::tests::mixed_transport_grid_crossing_replay_calibration` on the same binary. Each source panel's SHA-256 is retained in the [placed caller report](2026-09-27-plan01-placed-caller-server.md).

The five raw logs are retained under ignored `target/plan11-scope-calibration/`; in table order their SHA-256 hashes are `3165e19bbbbb0e00b621deaa1297bb518ca5392e46ab4be6eddb34cd992cbb6a`, `b8790a23f2151f4285f074a47372c5b14c9cfc1732614eaa6deca0e75d8e6b42`, `0e57349db11dea616279abcde9e61db7de2a6f489ce7e2146890b2dbd66ba46c`, `5bd706480585404808af98a6c5a57dff2e99ea274c32ec9ed725b936e2fda14a` and `a90d6ee72ddc9195ff8e4ee046184177276181d1d88cb1c374ff924369f0fa0`. No server was used.
