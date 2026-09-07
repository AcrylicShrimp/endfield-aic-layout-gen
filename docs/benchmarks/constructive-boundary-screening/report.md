# M2: dense boundary accessibility screening

## Result and next decision

M2 is complete. Baseline is `a7f29b4` (M1), not the pre-M1 historical runtime. Five paired
unprofiled release runs preserve every accepted-node/search-choice signature while reducing
median full CLI latency by 54.4% and median construction latency by 62.2%.

| Heavy Xiranite, five samples each | M1 baseline median [min, max] | M2 median [min, max] |
| --- | ---: | ---: |
| Full CLI with JSON/HTML (s) | 13.362 [13.325, 13.878] | 6.091 [5.754, 6.108] |
| Construction (s) | 11.697 [11.646, 11.702] | 4.417 [4.125, 4.427] |
| Peak RSS (decimal MB) | 284.951 [284.000, 285.458] | 284.164 [283.689, 284.754] |

Timing ranges do not overlap. Peak RSS ranges overlap; there is no memory-growth trade-off in
this workload. The faster fifth candidate sample is retained, not discarded. These are same-host
measurements, not a universal runtime guarantee.

Both variants complete 59 facilities, 89 internal and seven external requirements, 47 accepted
steps and 178 A* calls. The natural footprint remains 165 x 140, never an input constraint.
All 14 runs per workload (five unprofiled and two profiled per binary) have identical witness/
choice signatures. For Heavy Xiranite:

- Final node SHA-256: `acd24c232943a16a53455e6de1b6aea8d9bd95137ab5302568f5b15e9f2e6009`.
- Accepted-node/search-choice SHA-256: `8349374789095c182efea871db56b73b022e940361b0d5ef94f9d13707862077`.

M3 is next: independent network validation is now the largest exclusive construction operation,
at 1.539-1.542 s across candidate profiles (about 35% of construction). Crossing-option generation
is next at 1.084-1.085 s across all callers. Inspection confirms repeated whole-segment scans in
the validator's forward/reverse reachability loops and per-cell component-list scans, but these
are implementation candidates, not yet separately timed causes. M3 should instrument those
suboperations before choosing the smallest useful index. Any validator index must be rebuilt
independently from the witness, not borrowed from constructor acceptance state. No validator
rewrite, crossing rewrite or parallelization is bundled into M2.

## Separate profiles and observed variability

The first separately profiled baseline was faster than its unprofiled distribution (9.756 s
construction vs 11.646-11.702 s). Several operations, not only screening, were faster. The cause
was not established; do not attribute that change to profiling or to M2. An additional pair in
opposite order reproduced the baseline at 11.927 s and candidate at 4.441 s. Both profiles are
retained in [summary.json](../../../artifacts/benchmarks/constructive-boundary-screening/summary.json) and [profile-repeat.json](../../../artifacts/benchmarks/constructive-boundary-screening/profile-repeat.json). Neither is
mixed into the five-run unprofiled distributions.

| Operation | Baseline profile 1 / repeat (s) | M2 profile 1 / repeat (s) |
| --- | ---: | ---: |
| Boundary screen, inclusive, 92 calls | 6.976 / 8.478 | 1.180 / 1.175 |
| Independent validation, 200 calls | 1.343 / 1.547 | 1.539 / 1.542 |
| Crossing options, all 867 calls | 0.902 / 1.092 | 1.085 / 1.084 |
| Dense occupancy setup, 92 calls | N/A | 0.00232 / 0.00236 |
| Dense flood, 689 calls | N/A | 0.305 / 0.302 |

The repeat gives an 86.1% inclusive screening reduction. Do not sum nested inclusive operations:
crossing generation inside the screen already accounts for about 0.82 s of the new screen total.
New subspans change the parent's exclusive-time meaning: the candidate screen's 0.052 s exclusive
time excludes flood and setup, whereas the old screen's exclusive time included its hash flood.

Both candidate profiles record 689 commodity screens, 7,509,261 visited cells, 30,037,044 neighbor
probes and 629,525 in-envelope crossing hops. Across 92 workspace builds, envelope cells sum to
1,157,986 and auxiliary bytes to 11,579,860; those are aggregate allocations, not simultaneous
memory. Peak live workspace capacity is only 258,100 bytes. `peak-workspace-bytes` is a maximum;
other `work` fields are sums. Profile schema remains 1 and scope remains calling-thread-only.
The baseline does not contain these new work counters; controlled tests establish set/work
equivalence rather than inventing a baseline full-graph counter.

Full output remains enabled: approximately 96.68 MB JSON and 41.03 MB HTML. Candidate profile 1
attributes 0.640 s to report serialization/writing, 0.970 s to HTML rendering and 0.018 s to HTML
writing. No output history is removed. Stdout stays one short completion JSON line, approximately
250 bytes with these experiment paths.

The smaller seed-cycle fixture also retains its exact witness (two facilities, two internal and
one external requirement, two steps, three A* calls). Unprofiled full-CLI medians are 31.116 ms
baseline and 30.218 ms candidate, with overlapping ranges 30.299-34.143 and 29.658-31.614 ms.
Construction is 2-3 ms versus 2 ms at report precision; RSS medians are 8.339 vs 8.356 MB.
No material small-case regression is observed; the overlapping short runtimes do not establish
a useful small-case speedup.

## Implementation and equivalence contract

The [stage contract](../../todos/2026-09-05.07-boundary-screening.md) was defined before code changes.
Screening now builds one candidate-local byte occupancy grid with independent facility, belt and
pipe flags. A second byte grid and dense FIFO queue are reused across sorted commodity screens.
Reset copies immutable occupancy and clears the queue, discarding all previous crossing/visited
flags. No state or cached answer survives a candidate mutation. Crossing-option generation remains
the original runtime-catalog-backed function, called separately for each (transport, item).

The flood performs the same four directed neighbor transitions in the same order. Consecutive
crossing cells are skipped only on the compatible independent axis. They are not enqueued or
marked reachable merely because a route can pass through them. Same-commodity transport occupancy,
illegal crossing axes, facilities and bridge/component exclusions retain their existing semantics.
The root (-1, -1) is marked even if occupied, exactly as in the reference.

The inclusive envelope is unchanged: placement ends and network cell ends plus three, with lower
bound -1. Only obstacles inside this envelope need materialization. A crossing step leaving the
envelope cannot return because its direction is constant, so stopping at the border preserves the
reachable set. No margin is enlarged, no area is clipped from the flood, and no boundary check is
removed. Checked i128 coordinate conversion avoids signed overflow in extent/index calculations.

All successful screens return the same sorted/deduplicated blocked requirement IDs. The old hash
flood is removed from production and exists only as a test oracle. Production has one screening
path with explicit immutable occupancy and resettable scratch, without per-commodity occupancy-set
clones or per-neighbor hash/tree queries. Candidate enumeration, A*, independent validation and
original-wiring audits remain unchanged. No parallel worker or global exact fallback is introduced.

Auxiliary dense grid memory is two bytes plus one usize per envelope cell (10 bytes on this host).
Checked size arithmetic, a 64 MiB auxiliary allocation guard and fallible reservation prevent an
unbounded dense allocation. This is an operational resource guard, not a game limit or caller W/H
constraint. It does not include the borrowed candidate or temporary crossing-option map; peak RSS
measures the process separately. Exceptional outcomes have distinct codes:

- `production-boundary-coordinate-overflow`
- `production-boundary-workspace-budget-exhausted`
- `production-boundary-workspace-allocation-failed`

These reject the uncommitted candidate without fabricating blocked requirements. Rejection counts
retain the code; if growth exhausts candidates, a structured error records the affected connection
and screening failure alongside the ordinary growth diagnostic. Prior validated geometry is kept.
An allocation/representation failure is not a reachability or global infeasibility conclusion.
This exception contract replaces possible unchecked allocation/overflow failure, not a legal-path
heuristic. Ordinary comparisons remain inside the guard with unchanged routing/search budgets.

## Tests and controlled scaling

External `constructive-boundary-screening.json` fixtures compare every visited coordinate against
the former hash traversal for enclosures/openings, horizontal/vertical/wrong-axis crossings,
consecutive and border crossings, negative exterior coordinates, an occupied root and an excluded
domain. Each workspace is queried with crossings enabled, disabled, then enabled again; a different
transport layer is also screened to detect stale occupancy or visit flags.

Generated chain, branch and atomic-cycle candidates are compared at every accepted step, translated
to positive/negative coordinates, with duplicated boundary domains for different commodities,
changed network transport layers and deliberately obstructed attachment points. These mutations
are query fixtures, not new claims of valid game layouts. The full blocked-ID list must match the
test-only original implementation. Resource/index guards and coordinate-overflow errors are tested
separately from inaccessible boundaries. Existing bridge, capacity and independent-validator tests
remain in force.

Controlled empty-grid scaling asserts exact reachable-set equality, four neighbor probes per
visited cell and linear allocated capacity. These are deterministic work/storage checks, not
microbenchmark timing claims:

| Fixture maximum coordinate, both axes | Visited cells | Neighbor probes | Workspace bytes |
| --- | ---: | ---: | ---: |
| 16 | 324 | 1,296 | 3,240 |
| 64 | 4,356 | 17,424 | 43,560 |
| 256 | 66,564 | 266,256 | 665,640 |

The algorithm still explores the same cells; the improvement removes hashing, repeated occupancy
construction and coordinate-map lookups, rather than changing reachability or pruning candidates.

## Reproduction protocol

Build both revisions in release mode before timing. The baseline binary was copied before source
edits; its SHA-256 is `949c874c954caeb6fff4bb1e92cf9775e97948ce21844a54f581cec5d2608bcc`.
Use the [paired harness](run.mjs) from the repo root with a new output directory:

```sh
node docs/benchmarks/constructive-boundary-screening/run.mjs /absolute/path/baseline-aic-cli target/release/aic-cli /absolute/path/new-experiment-directory
```

The macOS harness uses `/usr/bin/time -lp` for CPU/RSS. Five unprofiled pairs alternate order,
followed by one separately profiled pair, for Heavy Xiranite and the existing seed-cycle workload.
All samples are retained. Compiler/test processes must finish before any timed run. An earlier
full-factory correctness smoke that overlaps tests is explicitly excluded from timings.

The committed evidence records input/binary hashes and the Darwin 25.6.0 arm64, 10-logical-CPU
host with stable Rust 1.98.0. The additional diagnostic pair is reproduced with the same command
and a different new output directory, appending `--profile-only`. That mode runs baseline then
candidate, opposite the original profile pair; it does not fabricate unprofiled distributions.

Catalogs, source plans, target seed, 256-step/256-placement/4,096-route budgets and full JSON/HTML
outputs are identical across binaries. Stdout is checked for one short completion JSON line and
never receives the full report. Profile JSON is captured from stderr to experiment files. The
signature includes final and intermediate nodes, step statistics/attempt counts, failed statistics,
rejection reasons and diagnostics, excluding elapsed time and profiler internals.

The accepted constructive design and AGENTS.md remain in force: natural-footprint connected growth,
transactional acceptance and independent validation. This slice optimizes screening only; M3 must
re-profile before selecting another serial operation, and M4 retains the later parallelism decision.

## Verification and activity

- Passed `cargo fmt --all`, `cargo check --workspace`, `cargo test --workspace --quiet`, release
  CLI build, manual full CLI smoke, 28 measured CLI invocations, harness syntax and diff checks.
- Workspace tests: 483 passed, two existing ignored tests (including M1's explicit microbenchmark).
  The data crate has 419 passing tests. No ignored test is counted as passing here.
- An initial test-oracle visibility error was corrected. An added three-source synthetic fan-in
  fixture failed its prerequisite construction assertion, not the new/old screening comparison;
  it was replaced by multi-commodity domain variants of the already-complete chain/branch/cycle
  fixtures. The full workspace suite was rerun and passed after that correction.
- M2's accepted-state, search-counter and memory gates pass. Updated the milestone tracker;
  validator suboperation measurement is the next M3 work unit.
