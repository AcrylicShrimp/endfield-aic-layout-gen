# M5: complete-construction runtime checkpoint

## Result and decision

The M0-M5 runtime phase is complete. The preserved M0 binary (`f8ce096`, before M1) and the final
M4 default (`f5b18ee`, at most four analysis workers) were remeasured in paired release runs on
the same host. No layout/search semantics changed. Next implement the bounded route transaction
specified in the [quality contract](../../designs/2026-09-05.00-transactional-route-improvement.md),
tracked as [Q1](../../todos/2026-09-05.11-transactional-route-improvement.md).

| Heavy Xiranite, five unprofiled samples each | M0 median [min, max] | M4 default median [min, max] |
| --- | ---: | ---: |
| Full CLI, JSON and HTML enabled, s | 23.221 [20.997, 23.320] | 4.828 [4.782, 4.842] |
| Construction, s | 21.521 [19.316, 21.697] | 3.140 [3.135, 3.141] |
| Peak RSS, decimal MB | 284.721 [284.590, 285.131] | 286.212 [285.786, 286.671] |

Median construction is 6.85 times faster (85.4% lower latency); full CLI is 4.81 times faster
(79.2% lower latency). Median peak memory rises 1.491 MB (0.52%). All samples are retained,
including the faster M0 round one: its 19.316 s planner / 20.58 s user time is below other baseline
runs, with unchanged witness and search work. This measurement does not identify why it ran faster;
even its minimum remains far above the candidate maximum. No outlier was removed or replaced.

Small seed-cycle CLI medians are 31.055 -> 31.122 ms, with overlapping 30.778-34.313 and
30.031-32.235 ms ranges. Planner medians are 3 -> 2 ms at report precision. Median RSS is
8.290 -> 8.421 MB. This shows no material small-case end-to-end regression or speedup.

## Mechanism and remaining costs

One separate profile per variant follows the unprofiled samples; these explain costs, not the
latency distribution. Inclusive parent/child times must not be added.

| Operation | M0 seconds | M4 default seconds | Work / interpretation |
| --- | ---: | ---: | --- |
| Existing-network path queries | 10.260 | 0.196 | Same 79,779 queries |
| Path index construction | n/a | 0.046 | 217 immutable indexes |
| Boundary screening, inclusive wall | 8.295 | 0.454 | Same 92 screens |
| Independent network validation, inclusive | 1.554 | 0.987 | Same 200 validations |
| Physical A* | 0.243 | 0.242 | Same 178 route searches |
| Attachment enumeration, exclusive | 0.510 | 0.464 | Excludes indexed query/build children |
| Node cloning | 0.146 | 0.131 | Same 1,007 clones; not rewritten |
| Original coverage audit | 0.016 | 0.016 | Same 48 audits |
| JSON report serialization/write | 0.649 | 0.643 | Full report about 96.68 MB |
| HTML rendering, inclusive | 0.967 | 0.973 | Includes capacity inspection |
| HTML capacity inspection | 0.794 | 0.798 | Child of rendering, not an extra cost |
| HTML file write | 0.022 | 0.021 | Full HTML about 41.03 MB |

M1 queries examine 6,447,913 adjacent edges and dequeue 6,519,732 vertices; builds index 81,914
segments. M2/M4 boundary work totals 689 commodity screens, 7,509,261 visited cells, 30,037,044
neighbor probes and 629,525 crossing hops. M3 reachability checks examine 588,934 adjacent segments
and dequeue 601,094 vertices; the separate pre-index instrumented M3 baseline measured 270,092,646
full-vector probes. M0 has no such suboperation counters, so that older count is attributed to the
[M3 experiment](../constructive-independent-validation/report.md), not fabricated for this run.

Main M4 crossing generation has 178 calls; workers have 689, preserving M0's 867 total.
The 689 worker floods are separate from the main operation map. Profile scope is now
`calling-thread-with-separate-worker-aggregates`; worker wall sum is 1.342 s and dispatch/join
critical-path wall is 0.449 s. These overlap and must not be summed. Peak worker workspace is
1,290,500 bytes; maximum concurrently live workers is four. Native process CPU/RSS is recorded
separately in [summary.json](../../../artifacts/benchmarks/constructive-runtime-checkpoint/summary.json). See the [M4 matrix](../constructive-parallel-analysis/report.md)
for one/two/four/eight-worker distributions and default-selection evidence.

The next largest construction aggregate is independent validation, spread over cell, segment,
topology and connectivity checks. Attachment enumeration and boundary screening each cost about
0.45 s on the main critical path. JSON/HTML processing outside construction remains about 1.64 s;
most HTML time is capacity inspection, not writing bytes to disk. There is no evidence here that
A* rewriting, clone elimination, more workers, or skipping validation is the next useful product step.

Defer further runtime changes while establishing quality improvement. If faster rendering later
becomes a requirement, first profile/index its 0.798 s capacity inspection. Reducing cumulative
history size is an explicit output-contract decision, not a hidden performance optimization.
The report is still large because it retains cumulative construction states; stdout does not replay
that history. This checkpoint neither removes history nor claims the output-size issue is solved.

## Correctness and geometry checkpoint

All 24 new runs match every complete/intermediate node, placement attempt, search statistic,
rejection reason and ordered diagnostic after excluding elapsed/profile fields. Heavy Xiranite
retains 59 facilities, 89 internal/seven external requirements, 47 accepted states, 178 A* calls,
no unresolved requirements and successful independent validation/original-rate audit.

- Final node SHA-256: `acd24c232943a16a53455e6de1b6aea8d9bd95137ab5302568f5b15e9f2e6009`.
- Witness/search-choice SHA-256: `8349374789095c182efea871db56b73b022e940361b0d5ef94f9d13707862077`.
- Seed-cycle witness/search-choice SHA-256:
  `6527bc288e801abcfbf729631bbc370726ba83297887070cb73f5dfe188a2872`.

Observed geometry remains 165 x 140 (area 23,100), with 4,265 unique transport-layer cells. These
are measured outputs, not game limits or planner inputs. Facilities already span x=0..164 and
y=11..136; fixed external terminals include y=0. A route-only pass cannot reduce width while
those placements remain fixed. Q1 therefore establishes safe transactions and may improve tiles/
turns, but does not promise meaningful area compaction of this fixture. Facility/group relocation
and route repair require the next explicitly scoped quality contract, not a return to global exact search.

## Reproduction, verification and milestone closure

Host: Darwin 25.6.0 arm64, 10 logical CPUs; Rust stable 1.98.0. Both binaries are release opt-level 3.
[summary.json](../../../artifacts/benchmarks/constructive-runtime-checkpoint/summary.json) contains binary/input hashes, all 24 runs, CPU/RSS, output sizes,
separate profiles and five-sample distributions. The M0 binary hash matches the preserved original
[M1 baseline](../../../artifacts/benchmarks/constructive-indexed-paths/summary.json). The current binary is the same one tested
and measured for M4, not a rebuilt or instrumented-only candidate.

```sh
node docs/benchmarks/constructive-runtime-checkpoint/run.mjs /absolute/path/m0-aic-cli target/release/aic-cli /absolute/path/new-experiment-directory
```

The macOS [harness](run.mjs) alternates five unprofiled pairs then profiles each variant separately
on full/small inputs. Catalogs, source plans, seed, budgets and JSON/HTML mode are identical.
Compiler/tests finish before timing. Large raw output stays outside git; benchmark parsing/hashing
is outside the timed child. Every CLI stdout is one short completion JSON line, checked under 4 KiB.

M4's full verification remains applicable: formatting/check passed, 492 workspace tests passed
with two existing ignores, release build passed, default/invalid-worker manual CLI checks passed.
M5 changes no Rust behavior. Harness syntax, all 24 manual CLI executions/signatures, documentation
links and `git diff --check` pass for this documentation/measurement unit.

M1 (`a7f29b4`), M2 (`57904fa`), M3 (`86bd588`) and M4 (`f5b18ee`) each have a separate measured
implementation commit. M5 closes the runtime phase and supplies Q1's input/output, transaction,
validation, score, failure/report and verification contracts. Q1 implementation is not bundled here.
