# File-backed production reports and short stdout summaries

Date: 2026-09-05. Based on the operation-profile baseline at `dbb5ebd`.

## Outcome

`layouts construct-production` now prints one compact completion summary instead of serializing
the full construction report to stdout again. The existing full report and HTML files remain.
The measured complete Heavy Xiranite request still constructs exactly the same 59 facilities,
89 internal requirements, seven external requirements and 47 accepted states with 178 A* calls.

| Metric | Before, profiled run #1 | After, profiled run #1 |
| --- | ---: | ---: |
| CLI wall time | 30.629 s | 23.319 s |
| Planner time | 21.535 s | 21.633 s |
| JSON file serialization/write | 0.652 s | 0.648 s |
| Full stdout serialization / new summary | 7.392 s | 0.000083 s |
| Process system time | 5.68 s | 0.39 s |
| Stdout size | about 92 MiB | 248 bytes |

The observed end-to-end reduction is about 24%, or 7.3 seconds. The second profiled run takes
23.266 s, and the profile-disabled run takes 23.898 s. This is a small host-local sample, not a
universal throughput claim. The planner itself has not been optimized in this slice.

The post-cutover benchmark consumes the small stdout pipe to verify its contract; the historical
baseline redirected stdout to `/dev/null`. Full JSON-file output and HTML generation remain enabled
in both. The isolated operation timer and the system-time reduction support attributing the main
improvement to removing duplicate stdout serialization, rather than to a new search algorithm.

## Explicit CLI contract change

Stdout is now one newline-terminated compact JSON object:

```json
{"schema_version":1,"kind":"constructive-production-summary","status":"complete","success":true,"report_output":"factory.json","visualization_output":"factory.html"}
```

Consumers needing placements, networks, step history or full diagnostics must read
`--report-output`, not parse those fields from stdout. No compatibility switch for duplicate
full-report stdout output remains. The full construction-report schema is still 2.

- Completed and partially completed planner reports go to the required JSON file. A partial result
  has `success=false` and still identifies its written HTML when one exists.
- Early recipe/throughput/facility/wiring/item/transport/logistics validation failures write the
  native stage-specific report to the JSON file. Their stdout summary says `invalid-input`,
  `success=false`, `visualization_output=null`; it does not dump the diagnostics.
- Missing/invalid JSON inputs, concise localization setup errors and output-write errors remain
  path-specific errors on stderr. No completion summary is printed if required artifact writing
  fails. A previously written report may still exist after a later HTML-write failure.
- `--profile` still emits one small aggregate JSON record on stderr. Its former `report-stdout`
  operation is removed; the new `completion-summary` operation measures only the short summary.
- Other CLI commands retain their existing output behavior. This is not global stdout suppression.

The shared input loader now returns typed validation failures instead of writing to stdout itself.
Callers choose their diagnostic destination explicitly. This removes hidden output side effects
from input loading and lets production route failures to files without redirecting process-wide
stdout or introducing a compatibility layer.

## What is still expensive

The 92 MiB JSON file still contains the full accepted-state history; this slice does not replace
it with events or remove diagnostics. Generation remains sequential. Profiled after-cutover costs
remain about 10.3 s for 79,779 existing-network path searches and 8.2-8.3 s inclusive for 92 boundary
accessibility checks. A* still costs only 0.24 s for 178 calls. These are the next measured
construction targets; see the preceding runtime-profile report for code-level mechanisms and
parallelization considerations.

## Verification and reproduction

The OFF/ON/ON post-cutover runs have identical final-node and accepted-state/search-choice hashes
to the pre-cutover profile baseline. Final-node SHA-256 remains
`acd24c232943a16a53455e6de1b6aea8d9bd95137ab5302568f5b15e9f2e6009`.

```bash
cargo build --release -p aic-cli
node docs/benchmarks/constructive-runtime-profile/run.mjs /tmp/aic-summary-new-run
```

Wait for compilation/tests to finish before timing; the output directory must not exist. The
harness captures stdout, rejects more than 4 KiB on this fixture, verifies the single summary and
artifact paths, and compares every accepted node and all recorded search-choice counters. Raw
large artifacts stay outside the repository; `summary.json` retains the compact measurements.

Formatting, workspace check/tests and the release CLI runs pass. Five new CLI integration tests
cover early invalid catalogs (five catalog kinds), an invalid planner request, a partial factory,
missing input, and report/HTML write failures. They require a short summary or no completion claim,
as appropriate, and verify that the full diagnostics remain in the JSON file.

The slice was compared with AGENTS.md and the accepted constructive-planner design. It changes
only output ownership and diagnostics routing: no placement/routing decision, game rule, resource
bound, score priority, complete-factory acceptance or exact-research boundary changed.
