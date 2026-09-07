# M3: independent witness reachability

## Result and next decision

M3 is complete. Baseline is `57904fa` (M2). This slice changes only validation reachability and
adds profiling. Five paired unprofiled release runs show a 12.5% median construction reduction
and a 9.2% full CLI reduction, with identical witnesses and search choices.

| Heavy Xiranite, five samples each | M2 median [min, max] | M3 median [min, max] |
| --- | ---: | ---: |
| Construction (s) | 4.434 [4.420, 4.967] | 3.878 [3.836, 3.897] |
| Full CLI with JSON/HTML (s) | 6.109 [6.097, 6.652] | 5.546 [5.506, 6.016] |
| Peak RSS (decimal MB) | 283.951 [283.640, 284.115] | 285.245 [284.164, 285.393] |

Every candidate run is faster than its paired baseline. Both latency ranges are non-overlapping,
although the full-CLI gap is smaller than the construction gap due to output-time variability.
The first baseline construction and first candidate output phase were slower than later samples;
all samples are retained. Single-host measurements do not establish a universal speedup.

The result retains 59 facilities, 89 internal and seven external requirements, 47 accepted steps,
178 A* calls and the natural 165 x 140 footprint. Every final/intermediate witness and all recorded
search statistics, attempts, rejection reasons and diagnostics match across the 12 final paired/
profiled runs per workload, and also match the earlier instrumentation probe.

- Final node SHA-256: `acd24c232943a16a53455e6de1b6aea8d9bd95137ab5302568f5b15e9f2e6009`.
- Accepted-node/search-choice SHA-256: `8349374789095c182efea871db56b73b022e940361b0d5ef94f9d13707862077`.

There is a small measured memory cost: median RSS increases 1,294,336 bytes (0.46%). Inspection
confirms that the new adjacency/vertex/scratch allocations are local to one network and dropped
before the next network; no history-wide index is retained. The largest network among accepted
states has 1,492 cells and 1,492 segments, not a canvas-sized index. RSS includes code, output
history and allocator retention, so its exact delta is not attributed solely to live index bytes.
Accept this modest cost for the repeatable latency gain; do not claim memory is unchanged.

The seed-cycle workload also retains its exact witness: two facilities, two internal and one
external requirement, two steps and three A* calls. Full-CLI medians are 32.086 vs 31.977 ms,
with overlapping ranges 31.058-33.092 and 31.478-32.335 ms; both report 2 ms construction.
RSS medians are 8.323 vs 8.487 MB (an additional 0.164 MB). No useful small-case latency speedup
is claimed; there is no observed latency regression outside variability, but memory is not free.

M4 is next: make the bounded parallel-analysis decision using the new cost distribution. The
largest exclusive construction operation is now crossing-option generation at 1.090 s across
867 calls. Independent validation totals 0.988 s, now distributed across several smaller checks.
Candidate-local commodity screens remain a possible parallel workload, but their whole 1.177 s
includes about 0.822 s of crossing generation plus 0.300 s of flood, not two disjoint large costs.
Define worker controls, bounded scheduling, deterministic result order and worker-aware profiling
before testing one worker against multiple workers. Do not race growth commits or silently reuse
the current calling-thread-only profiler for worker-time conclusions. No M4 adoption is claimed.

## Final separate profile

[summary.json](../../../artifacts/benchmarks/constructive-independent-validation/summary.json) records five unprofiled pairs and one separate profiled pair per
workload, with binary/input hashes, host/toolchain, CPU, memory and complete operation records.

| Operation | Original / probe (s) | Indexed candidate (s) |
| --- | ---: | ---: |
| Independent validation, final paired profile | 1.528 | 0.988 |
| Reachability including index, instrumented original probe | 0.695 | 0.143 |
| Index construction, included in reachability | N/A | 0.101 |
| Reachability exclusive of index construction | 0.695 | 0.041 |
| Cell checks | 0.219 | 0.218 |
| Segment checks | 0.226 | 0.225 |
| Topology checks | 0.177 | 0.176 |

Original substage timings come from the pre-optimization instrumented probe, not the uninstrumented
final baseline binary. The other checked stages remain stable. Reachability including its index
falls about 79%; independent validation falls about 35% in the final pair. Parent/child inclusive
times must not be added. Reachability exclusive time also includes roots, resets and cleanup, not
only the inner traversal loop.

The same 601,094 vertices are visited while segment probes fall from 270,092,646 to 588,934
(about 459 times fewer). All 2,172 network validations still run. Local indexes process 300,547
vertices and 294,467 segments in aggregate; no checks are skipped or replaced with trusted flags.

Candidate report serialization/writing is 0.648 s, HTML rendering 0.959 s and HTML writing 0.018 s.
Full output remains about 96.68 MB JSON and 41.03 MB HTML; no history is dropped. Stdout stays one
short completion line of approximately 250 bytes with these experiment paths.

## Measure before choosing the change

The [contract](../../todos/2026-09-05.08-independent-validation.md) required a release probe of the
unchanged validator before any indexing. [probe.json](../../../artifacts/benchmarks/constructive-independent-validation/probe.json) compares the preserved M2 binary
with an instrumentation-only binary: its `candidate` label means the instrumented old algorithm,
not the final optimized implementation. Both full and seed-cycle witness signatures match.

| Instrumented original validator | Calls | Seconds |
| --- | ---: | ---: |
| Complete independent validation | 200 | 1.543 |
| Setup, placement/port projection and global membership | 200 | 0.126 |
| Cell occupancy and initial flow map | 2,172 | 0.219 |
| Segment membership, rate and flow accumulation | 2,172 | 0.226 |
| Terminal checks and terminal flow | 2,172 | 0.037 |
| Cell conservation, arms and components | 2,172 | 0.177 |
| Forward/reverse reachability | 2,172 | 0.695 |

Reachability is the largest suboperation (45% of validation). It performs 270,092,646 segment
probes for 601,094 dequeued vertices because each queue pop scans the whole network segment list.
Only this stage is rewritten. Component indexing, port projection and other validator checks are
deferred rather than bundled into an unmeasured rewrite. The remaining 0.062 s is parent-exclusive
work including final checks and cleanup; do not add the complete-validation parent to its children.

## Independent index and semantic preservation

Each network's reachability stage rebuilds forward/reverse adjacency from the witness's checked
cell set and segment vector. It assigns dense local vertex IDs and uses resettable seen/queue
storage. The topology borrows the immutable segment slice and lives only inside that network's
validation. It accepts no constructor path index, cached validity, capacity report or routing state.
The constructor's M1 index and the independent validator remain separate consumers of the witness.

All existing placement, membership, positive rate, adjacent-segment, duplicate, flow-conservation,
terminal, component identity/topology and bridge-axis checks remain in their original order.
Adjacency is constructed only after those network checks. Invalid input therefore still produces
the same first diagnostic; a duplicate arc or missing terminal is not silently normalized by the
index. Final cross-network bridge ownership/axis checks remain after all network-local checks.

Forward traversal starts from supply terminals; reverse traversal starts from demand terminals.
Each must cover every physical cell. The old whole-segment loop and ordered adjacency enumerate
the same directed transitions, so their reachable closure is identical. Duplicate roots can be
enqueued once because reachability is idempotent; this changes only redundant internal work, not
planner budgets or diagnostics. Scratch is reset before reverse traversal or another query.

Disconnected circulation remains invalid. A directed cycle connected to the required terminals
is not newly prohibited: this is the existing coverage rule, not a new acyclicity constraint.
The same `unreachable network geometry` diagnostic is returned at the same forward/reverse stage.
No W/H canvas, new game rule, validation shortcut, global exact fallback or parallel worker is added.

Index storage is O(V+E), based on existing sparse topology rather than coordinate extent. Coordinate
lookup during construction uses a BTreeMap; each traversal is O(V+E) after O(V) scratch reset. The
former production full-vector traversal is removed, leaving a dedicated topology/coverage routine.
The unchanged full validator is retained only in a test module as a diagnostic oracle.

Profile schema 1 gains grouped `validation-*` spans and `validation-index-build`. The reachability
parent includes index creation and cleanup; its exclusive time excludes the index-build child.
Bounded work fields count `vertices-visited`, actual `segment-probes`, `vertices-indexed` and
`segments-indexed`. Full-vector probes in the instrumented baseline and outgoing adjacency entries
in the candidate are explicitly distinguished; neither counter consumes a planner search budget.
All accounting remains calling-thread-only.

## Correctness and scaling gates

Every validator assertion in the existing network-test module now compares its complete result
against the original validator, including structured first diagnostics. This covers valid rotated/
translated fan-out, merge and independent-axis bridges, plus invalid components, overload, wrong
ports, duplicate bridge ownership and bad bridge axes.

Additional corrupted witnesses exercise duplicate facilities, unknown definitions, bad footprints,
membership and requirement mismatches, duplicate commodities/cells/arcs, empty cells, zero rates,
missing segments, wrong terminal identity and component references. Clearing the constructor's
capacity-report cache still validates. A separately injected balanced circulation passes preceding
flow/topology checks and is rejected specifically for unreachable geometry by both validators.

External `constructive-validation-queries.json` supplies single-cell, chain, branch, merge, connected
cycle, disconnected cycle, isolated cell, absent supply/demand and negative-coordinate/duplicate-root
cases. Queries compare reference coverage in both directions using empty, single, fixture and all-cell
root sets, repeatedly resetting scratch after success/failure and rebuilding after edge removal/order
changes. These topology fixtures test only reachability, not the validity of arbitrary game networks.

For an N-edge chain, each reference direction scans N(N+1) segment entries; indexed traversal
examines N outgoing entries while visiting the same N+1 vertices. Tests assert these exact work
counts and one independently built index for both directions at N = 8, 32, 128 and 512. They make
no timing assertions and do not confuse reduced scan work with fewer validation checks.

## Reproduction and scope

The [harness](run.mjs) compares prebuilt release binaries using identical normalized runtime
catalogs, source plans, target seed, 256-step/256-placement/4,096-route budgets and full JSON/HTML.
It runs five unprofiled pairs in alternating order, then a separately profiled pair on full Heavy
Xiranite and the existing seed-cycle workload. Compiler and test processes must finish before
timing. macOS `/usr/bin/time -lp` records CPU and peak RSS. Output directories must be new.

```sh
node docs/benchmarks/constructive-independent-validation/run.mjs /absolute/path/baseline-aic-cli target/release/aic-cli /absolute/path/new-experiment-directory
```

Appending `--profile-only` selects only diagnostic pairs; such samples are never pooled into the
unprofiled distribution. Large raw reports/HTML stay outside git. Stdout remains one short completion
JSON line; full reports use file I/O, with optional profile JSON captured from stderr. Signatures
cover final/intermediate nodes, step statistics and placement attempts, failures, rejected reasons
and diagnostics, excluding elapsed time and profiler internals.

The host is Darwin 25.6.0 arm64 with 10 logical CPUs and stable Rust 1.98.0. Both diagnostic and
final paired measurements ran only after compiler/tests finished. The final release build includes
the import cleanup after removing the production VecDeque traversal; the earlier intermediate
optimized build was not used in the final benchmark.

The accepted constructive-planner design and AGENTS.md remain unchanged: connected transactional
growth at a natural footprint, full independent witness validation and original-wiring rate auditing.
M4 remains a separate measured parallelism decision, followed by the M5 runtime checkpoint and
layout-improvement contract. This slice does not race growth commits or weaken valid-layout checks.

## Verification

Passed `cargo fmt --all`, `cargo fmt --all --check`, `cargo check --workspace`,
`cargo test --workspace --quiet`, `cargo build --release -p aic-cli`, harness syntax/diff checks
and real release CLI comparisons. Workspace tests: 487 passed, two existing ignored tests;
the data crate has 423 passing tests. Normal validation and independent original-wiring audits
remain active in every measured CLI run. Benchmark evidence includes four initial diagnostic
invocations and 24 final comparison invocations; timing samples are not mixed across those phases.
