# Complete-factory runtime profile

Date: 2026-09-05. Instrumentation-only slice based on `5cfe2ed`. No candidate-order, routing,
validation, catalog, construction-report or HTML-content change is intended.

## Result

The main construction costs are repeated existing-network path searches and future-boundary
accessibility screening, not A*. Duplicate stdout serialization dominates post-construction time,
not writing the JSON file to disk. The full-production path is sequential; parallel workers in
the older pipe-chain candidate evaluator and isolated exact research tools are not used here.

All three runs complete the same 59-facility, 89-internal-requirement, seven-external-requirement
factory. They have identical hashes of final geometry, every accepted node, candidate/route counters
and rejected-reason aggregates. Final-node SHA-256:
`acd24c232943a16a53455e6de1b6aea8d9bd95137ab5302568f5b15e9f2e6009`.
There are still 47 committed transactions and 178 A* calls.

## Measurement conditions

- macOS 26.6 arm64, Rust stable 1.98.0, `cargo build --release -p aic-cli`.
  The compiler invocation was checked for `-C opt-level=3`; this is not a debug benchmark.
- Finish compilation and workspace tests BEFORE the three sequential measured runs.
  `/usr/bin/time -lp` measures each CLI process; stdout goes to `/dev/null` in every run.
- Baseline OFF: 21.504 s planner / 31.099 s CLI, 24.87 user / 5.68 system seconds.
- Profile ON #1: 21.535 s planner / 30.629 s CLI, 24.90 user / 5.68 system seconds.
- Profile ON #2: 21.601 s planner / 30.726 s CLI, 24.98 user / 5.70 system seconds.
- Peak RSS is approximately 285 MB in each run. Planner differences are 0.03-0.10 s in this small
  sample. These observations do not establish a precise overhead percentage or a speedup.
- An earlier separate, pre-instrumentation baseline was 21.597 s planner / 31.11 s CLI with the
  same final node. A filesystem poll observed the report growing over about 0.65 s, subsequently
  confirmed by the explicit report-file timer.

## Construction breakdown

Times below are from profile ON #1. Exclusive time subtracts directly nested instrumented scopes,
so these rows can be added without counting crossing analysis twice. It still includes work of
uninstrumented callees; it is NOT instruction-level self CPU time.

| Operation | Calls | Inclusive seconds | Exclusive seconds |
| --- | ---: | ---: | ---: |
| Existing-network directed path search (`shared-path`) | 79,779 | 10.113 | 10.113 |
| Future-boundary accessibility (`boundary-escape`) | 92 | 8.328 | 7.498 |
| Independent network validation | 200 | 1.553 | 1.553 |
| Catalog crossing-option analysis | 867 | 1.094 | 1.094 |
| Attachment enumeration | 1,380 | 10.608 | 0.495 |
| Physical A* routing | 178 | 0.243 | 0.243 |
| Full node clone | 1,007 | 0.144 | 0.144 |

`shared-path` plus boundary accessibility INCLUDING its crossing analysis accounts for 85.6% of
construction time. These two scopes do not contain each other. Total A* is only about 1.1%.
Boundary refresh takes 8.298 s inclusive but 0.056 s exclusive: it mostly invokes attachment
enumeration, which in turn invokes `shared-path`. Adding these inclusive totals would be wrong.
Original-wiring coverage audit takes 0.016 s; port-demand analysis 0.049 s; capacity reporting
0.013 s. Maximum invocations and all 25 operation aggregates are in `summary.json`.

The second profile confirms the ranking: shared paths 10.160 s, boundary accessibility 8.327 s
inclusive, A* 0.243 s. This is evidence about this request, not a universal workload distribution.

### Code mechanisms to address next

1. `network::shared_path` searches again for every attachment anchor. At every dequeued cell it
   scans the entire segment vector to find outgoing edges. Build a per-network adjacency index
   preserving segment order first; subsequently consider reusing traversals from identical roots.
   Preserve tie-breaking and capacity semantics when reusing paths; reverse BFS is not automatically
   equivalent to the current source-rooted path choice.
2. `blocked_boundaries` reconstructs occupancy/crossing data and runs an exterior flood fill for
   each unfinished commodity. Native stacks also show hash insertion and hashing in this path.
   Evaluate compact occupancy/visited arrays or reusable search workspaces on the naturally derived
   envelope, then shared immutable indexes. Do not remove the accessibility check or crossings.
3. Network validation still has per-cell full-segment scans. Share immutable indexes within one
   validation call, while keeping the independent geometry/flow and original-wiring checks.
4. Commodity-specific screening and network-local checks are potential parallel work over a fixed
   candidate. Coordinate global overlap checks and deterministic diagnostics separately. Growth
   commits themselves depend on the previous valid state. Candidate racing also changes the
   first-feasible ordering and operation-budget accounting unless explicitly coordinated.

This slice measures these opportunities; it does not yet implement their optimization. Node copies
are measured and are not a leading bottleneck here. A* parallelization and clone removal should
not be selected ahead of the much larger measured costs.

## Output breakdown and volume

| Pipeline operation | Seconds, run #1 | Seconds, run #2 |
| --- | ---: | ---: |
| Input loading | 0.011 | 0.011 |
| JSON report file serialization/write | 0.652 | 0.643 |
| Duplicate JSON stdout serialization/write | 7.392 | 7.435 |
| HTML construction | 0.974 | 0.972 |
| HTML file write | 0.021 | 0.020 |

The profile interval also includes small uninstrumented setup/destruction costs. It excludes CLI
startup and writing the profile itself. The external wall timer includes those costs. HTML
construction includes 0.799 s of capacity-inspection generation in run #1.

The previously stored report is 92.197 MiB pretty JSON, 31.171 MiB after removing whitespace.
On the compact representation, `steps` is 29.884 MiB and `final_node` only 1.285 MiB. This is not
an exhaustive trial log: it is 47 cumulative accepted-state snapshots plus the final state and
diagnostics. The snapshots repeat 83,221 transport segments and another 83,221 capacity-segment
records, compared with 4,368 of each in the final state. The pretty file has 3,592,462 newlines.

The file path already uses `BufWriter`. Stdout serializes the same pretty report again directly
through `StdoutLock`; samples show repeated stdout buffer flushes and kernel `write` calls even
with stdout redirected to `/dev/null`. The measured 7.4 s is not a 100 MB disk-throughput limit.
The next output-contract slice should separate a final artifact/summary from opt-in history
events (for example buffered JSONL changes and explicit checkpoints). Streaming unchanged full
snapshots alone does not remove their duplication or serialization cost. This slice deliberately
keeps existing output contents to preserve the baseline.

## Profiling interface and native evidence

Append `--profile` to `layouts construct-production`. One compact schema-1 JSON aggregate is
written to stderr; stdout and the ordinary report remain unchanged. The record has a monotonic
wall clock, explicit `calling-thread-only` coverage, and per-operation call count, inclusive,
exclusive and maximum nanoseconds. It contains no geometry, individual samples or event history.
Workers are NOT implicitly collected; future parallel implementations must make that coverage
explicit before interpreting these totals as whole-program work.

`native-sample.txt.gz` preserves the earlier independent CPU stack sample of the uninstrumented
release binary: 205 main-thread observations, including 143 under construction, 49 under stdout
serialization, seven under report-file serialization and six under HTML generation. `shared_path`
leads leaf-stack observations (60); kernel `write` is next (46). Sampling is coarse, and optimized
inlining hides `blocked_boundaries` inside the connection-acceptance closure. Do not convert these
counts into precise milliseconds or confuse them with the explicit wall timers. The installed
command-line developer toolchain cannot run Instruments (`xcrun xctrace` is unavailable).

## Reproduction and verification

```bash
cargo build --release -p aic-cli
node docs/benchmarks/constructive-runtime-profile/run.mjs /tmp/aic-profile-new-run
```

The output directory must not exist. The harness runs OFF/ON/ON, keeps raw reports outside the
repository, and checks hashes of all accepted states and operation counters before saving its
summary. To obtain an independent sample, attach `/usr/bin/sample PID 40 5 -file SAMPLE.txt`
to a separately launched release CLI, not to a timed baseline run. The harness optionally accepts
that sample path as its second argument and preserves it compressed.

`cargo fmt --all`, `cargo check --workspace`, `cargo test --workspace`, and CLI help/profile runs
pass. Tests cover deterministic nested/recursive accounting, disabled collection, calling-thread
isolation, nested-capture rejection, panic cleanup, CLI parsing, and unchanged construction
witnesses/counters with profiling enabled. A manual missing-catalog run returns exit 1, emits its
input-loading profile, and preserves the offending file path in its error.

The slice was checked against the accepted constructive-planner design and AGENTS.md: no global
exact fallback, pruning, output-footprint constraint, weakened validation, candidate-order change,
or premature layout-score optimization was introduced.
