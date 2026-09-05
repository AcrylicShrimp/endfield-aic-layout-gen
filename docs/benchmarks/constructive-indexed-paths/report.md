# M1: indexed existing-network path queries

## Result and decision

M1 is complete. Ordered adjacency removes the dominant existing-network query cost without changing
the factory, any accepted intermediate node, placement-attempt records, or budgeted search counters.
Proceed next to M2 boundary accessibility screening. Do not add root-result caching now: queries
and index construction together account for only 0.251 s of the separately profiled 11.840 s
construction. Accessibility accounts for 8.407 s inclusive (71%) and is the demonstrated next target.

| Heavy Xiranite, five unprofiled release runs each | Baseline median [min, max] | Indexed median [min, max] |
| --- | ---: | ---: |
| Full CLI, JSON and HTML enabled (s) | 23.419 [23.325, 24.007] | 13.400 [13.383, 13.801] |
| Construction (s) | 21.754 [21.636, 22.321] | 11.718 [11.504, 12.158] |
| Peak RSS (decimal MB) | 284.705 [284.590, 285.491] | 285.114 [284.328, 285.458] |

Median full CLI latency falls 42.8%; construction falls 46.1%. Timing ranges do not overlap.
Peak memory ranges overlap, with a 0.410 MB median increase; there is no large memory trade-off.
These results describe this host and workload, not a universal runtime guarantee.

## Contract and implementation

The [pre-implementation contract](../../todos/2026-09-05.06-indexed-network-paths.md) scopes the
change to directed path queries inside attachment enumeration. Each eligible immutable network
gets one sparse coordinate index and ordered adjacency, reused across its terminal/anchor queries
within that enumeration. The index borrows the segment slice and cannot survive its mutation.
Dense predecessor and FIFO queue storage is reused, with predecessor state reset before traversal.
There is no full-grid allocation, coordinate clipping, root-result cache, or reverse-rooted BFS.
Memory is O(V + E); index construction uses O((V + E) log V) coordinate-map operations. Queries
reset O(V) scratch, examine only outgoing reachable edges and reconstruct the selected path.

Path equivalence follows from preserving FIFO traversal, original segment order within each
source's outgoing edges, and first-discovery predecessor selection. Thus the same segment-index
path wins equal-length ties, including duplicate edges, cycles and convergence. Equal endpoints
still yield an empty path even when absent from the network; missing distinct endpoints and
unreachable destinations still yield no path. Capacity, terminal and component checks remain in
their original caller positions, using the same original segment indices.

The old full-vector traversal is removed from production and retained only as the differential
test oracle. The query implementation now separates immutable topology from reusable search
scratch instead of rediscovering adjacency and allocating a coordinate tree on every query.
No dependency, game rule, candidate pruning, budget, validation shortcut or parallel worker is
introduced. This preserves AGENTS.md and the accepted constructive-planner design: connected
transactional growth, independent validation, natural footprint and no global-exact fallback.

Profiling adds `path-index-build` and optional bounded `work` maps to operation records in schema 1.
Stable counters are `vertices-indexed`, `segments-indexed`, `vertices-visited`, and `edges-examined`.
Visited vertices mean dequeued BFS vertices (including an equal-endpoint query); examined edges
mean outgoing adjacency entries inspected. Missing indexed endpoints return before traversal.
Counters are aggregated after each invocation, not through thread-local access in the inner loop;
they never consume or alter planner search budgets. Profile scope remains calling-thread-only.

## Separate operation profile

Each variant was profiled once after its five unprofiled runs. These observations explain the
change; the repeated unprofiled distribution above establishes end-to-end improvement.

| Operation | Calls, baseline / indexed | Baseline seconds | Indexed seconds |
| --- | ---: | ---: | ---: |
| Shared path | 79,779 / 79,779 | 10.241 | 0.204 |
| Path index build | 0 / 217 | N/A | 0.048 |
| Attachment enumeration, inclusive | 1,380 / 1,380 | 10.760 | 0.718 |
| Boundary escape, inclusive | 92 / 92 | 8.497 | 8.407 |
| Boundary escape, exclusive | 92 / 92 | 7.681 | 7.572 |
| Independent network validation | 200 / 200 | 1.564 | 1.582 |
| A* | 178 / 178 | 0.249 | 0.247 |

Do not sum nested inclusive times. Indexed queries examine 6,447,913 outgoing edges and dequeue
6,519,732 vertices. Index builds process 81,914 segments and 82,087 vertices in aggregate.
The unchanged native baseline has no scanned-edge counters; no fabricated baseline work count is
inferred from time. Controlled tests below directly establish the eliminated quadratic scan.

Candidate report serialization/writing is 0.633 s, HTML rendering 0.942 s and HTML writing 0.019 s.
The full report remains approximately 96.68 MB and HTML 41.03 MB; output history was not reduced.
Every CLI invocation emits only one short completion-summary JSON line to stdout (under 4 KiB,
approximately 250 bytes with these paths), with full diagnostics in the report file. Opt-in profile
JSON is captured from stderr into the benchmark artifacts. There is no full-report stdout replay.

## Correctness and smaller/scaling cases

Both binaries complete Heavy Xiranite with 59 facilities, 89 internal and seven external requirements,
47 accepted steps and 178 A* calls. The natural footprint remains 165 x 140, an observed output,
not an input bound. All twelve runs have identical accepted-node/search-choice signatures after
excluding elapsed time and profile data. The comparison includes every step's node, statistics and
placement attempts, final node, failed statistics, rejected reasons and diagnostics.

- Final node SHA-256: `acd24c232943a16a53455e6de1b6aea8d9bd95137ab5302568f5b15e9f2e6009`.
- Witness/search-choice SHA-256: `8349374789095c182efea871db56b73b022e940361b0d5ef94f9d13707862077`.

The existing seed-cycle CLI fixture also has twelve identical witness/choice signatures: two
facilities, two internal and one external requirement, two steps and three A* calls. Five-run CLI
medians are 31.081 ms baseline and 30.808 ms indexed, with overlapping ranges of 30.326-33.327 ms
and 30.306-31.685 ms. Construction is 2-3 ms at report precision for both. Median RSS is
8.290 vs 8.372 MB. This small case shows no material end-to-end regression; it is too short/noisy
to claim a useful overall speedup. Its separate path-query plus build profile falls from 155 to
78 microseconds, but that single observation is not a latency-distribution claim.

External `constructive-path-queries.json` fixtures cover empty graphs, chains, branch/convergence
ties, directed cycles, disconnected and negative coordinates, duplicate edges and self-loops.
Tests compare exact segment vectors for every endpoint pair and every edge-order permutation,
repeat queries in reverse order to exercise scratch reset, and rebuild after segment removals.
Existing capacity, bridge-axis/crossing, corrupted-witness, structured-failure and growth regressions
remain unchanged and pass in the workspace suite.

For a directed N-edge chain, the reference examines N squared segment entries while the indexed
query examines N outgoing entries; both dequeue N + 1 vertices and return exactly the same path.
The release-only microbenchmark includes one index construction and 64 queries per sample, with
five alternating-order samples per variant. It uses the test-only reference with work counters,
so it supplements rather than replaces the native whole-program comparison.

| Chain edges | Reference median for 64 queries (ms) | Indexed, including build (ms) |
| --- | ---: | ---: |
| 8 | 0.172 | 0.133 |
| 32 | 0.510 | 0.128 |
| 128 | 3.958 | 0.365 |
| 512 | 41.754 | 1.267 |

The smallest samples are sub-millisecond and variable; do not generalize their ratio. The scaling
trend and exact examined-work counts support the mechanism independently of the full graph.

## Measurement and reproduction

Baseline is `f8ce096`, copied from its verified release CLI before source edits. Candidate is this
slice. [summary.json](summary.json) records both binary hashes, input hashes, host, toolchain,
all 24 run records, separate profiles and distributions. [scaling.json](scaling.json) records all
40 operation samples. The host is Darwin 25.6.0 arm64 with 10 logical CPUs, Rust stable 1.98.0.
Both binaries use release optimization. No compilation or test process overlaps timed runs.
An earlier correctness smoke run overlapped compilation and is explicitly excluded from timings.

The [harness](run.mjs) alternates baseline/candidate ordering for five unprofiled pairs, then runs
one separately profiled pair for each workload. It uses identical runtime catalogs, source plans,
target seed, budgets (256 steps, 256 placements, 4,096 route searches), and full JSON/HTML outputs.
All five samples are retained; no slow sample is discarded. Wall time covers process execution
and output, not subsequent benchmark parsing/hashing. Native `/usr/bin/time -lp` measures child
CPU and peak RSS; this harness is macOS-specific. Large raw artifacts stay in the experiment
directory outside git; durable structured evidence is committed here.

After preparing both release binaries from their respective revisions, run from the repo root:

```sh
node docs/benchmarks/constructive-indexed-paths/run.mjs /absolute/path/baseline-aic-cli target/release/aic-cli /absolute/path/new-experiment-directory
cargo test --release -p aic-data profile_path_scaling -- --ignored --nocapture --test-threads=1
```

The output directory must not exist. The scaling test runs only when explicitly requested in
release mode; regular tests make correctness/work assertions, never wall-time assertions.

Verification completed: `cargo fmt --all`, `cargo fmt --all --check`, `cargo check --workspace`,
`cargo test --workspace`, `cargo build --release -p aic-cli`, the explicit release scaling test,
`node --check` for the harness, `git diff --check`, and full/small manual release CLI runs.
The data crate reports 414 passed / two ignored in the normal suite; one ignored test is this
release benchmark and was subsequently run successfully. Other workspace suites also pass.

M2 must first define candidate-local occupancy/crossing workspace lifetime, checked coordinate
conversion, resource guards and exact blocked-requirement equivalence. M1 does not implement that
next stage or claim that screening can be removed. Validation and parallelism remain later measured
decisions in the [milestone roadmap](../../todos/2026-09-05.05-fast-construction-milestones.md).
