# Deferred successful-candidate materialization

Date: 2026-09-06. Status: verified performance improvement, unchanged complete factory geometry.

## Outcome and measurement

The same complete heavy-xiranite result remains 112x88, area 9,856, with 2,299 transport tiles and
256 turns. All 59 facilities, 131 recipe terminal margins and seven original external obligations
still pass independent validation. This slice improves execution cost, not layout quality.

Directly timed every structuredClone call using an optional preload. Ran three fresh Node processes
per phase with the same retained 53-facility witness, simple corridors, two-sided placement and trial
budget. No workspace tests or other benchmark jobs were intentionally overlapped with these nine
measurements. Phases were sequential, not interleaved; OS/thermal variation is not controlled.

| Phase | Median closure CLI time | Median generation | Full witness/candidate copies | Port copies |
| --- | ---: | ---: | ---: | ---: |
| Before | 4.972 s | 3.482 s | 149 | 8,867 |
| Private append-only drafts | 3.558 s | 2.233 s | 43 | 8,867 |
| Drafts plus retained port cache | 3.271 s | 1.921 s | 43 | 956 |

Local observed median closure time falls 34.2%, and generation time 44.8%. These are closure-only
times starting from an already constructed 53-facility witness, not full fresh-factory latency.
Both sides use the same instrumentation. Reports and raw timing paths are in `copy-timings.json`.
The previous coarse CPU profile pointed at copying; these call counts/timers confirm the mechanism.

The case has 148 placement trials but only 42 successfully constructed candidates. Previously each
trial deep-copied the whole retained witness, plus one final-output copy. Now only the 42 successful
candidates and final-output transaction receive full copies. The three before runs spend roughly
1.82-1.95 s copying full witnesses; final successful materialization plus output copying costs about
0.65-0.66 s. Port projection copies fall from roughly 0.28-0.29 s to 0.033-0.035 s.

## Ownership contract

Each trial has its own placement list, utilization map, network objects and appendable network
arrays. Existing cells, segments, terminals, component geometry and rate DTOs are read-only during
the trial. The existing connect/append implementation only pushes new entries into private arrays;
it never edits retained nested objects. Failed trials discard their drafts without materialization.

Before returning a successful candidate, deep-clone the whole candidate, including witness, projected
ports and route diagnostics. No mutable object is shared with inputs, catalogs, graph or sibling
candidates. This is an internal implementation cutover, not a selectable copy mode or compatibility
layer. It removes unnecessary failed-trial copying while keeping a simple detached output contract.

Project retained facility ports once per growth call. Only the new placement is reprojected for each
trial. Those cached ports remain private/read-only until detached by successful materialization.
The final-output transaction remains unchanged. Candidate/trial order, search bounds, score ranking,
port selection, route choices and validation semantics are unchanged. The run report adds draft and
materialization counts/times, separately from total generation time.

## Fidelity checks

- Replayed the complete 16-packing, two-corridor-family portfolio: all 954 candidate witness hashes,
  failures, trial counts, selected candidate indices, validation diagnostics and final selected
  witness bytes match across all 32 closures. Twelve closures still complete; others stop at exactly
  the same bounded frontiers. `copy-comparison.json` records the comparisons.
- The default forward control retains all 37 candidate hashes and its 10,296-area result unchanged.
- Frozen-input tests exercise successful and late-failing drafts. A deliberately excessive input
  rate forces partial append work followed by failure without changing the input. Every reachable
  object in a returned candidate is checked against the input/catalog/graph and sibling object sets.
  Mutating returned cells/rates/endpoints/components/placements/utilizations/ports cannot affect them.
- The comparator ignores timing fields but detects trial changes, candidate hash changes, changed
  selected witness bytes and vacuous empty portfolios. It does not certify arbitrary invalid inputs;
  independent geometry/coverage validation remains a separate mandatory gate.
- 60 related geometry tests and one comparator test pass. Workspace check passes; 524 Rust tests
  pass with two existing ignores. Eight independent complete-factory audit expectations pass.
- Re-rendered the unchanged complete result and inspected it personally. Browser zoom/pan/Fit and
  six viewport/theme checks pass. Large internal voids remain visible; no area improvement is claimed.

The full regression portfolio took 67.93 s versus the earlier 87.37 s observation, with validation
running concurrently for both. That is supporting context, not a controlled portfolio speedup claim.
The three-run single-closure matrix is the reported timing comparison.

## Reproduction and next work

Use the existing closure CLI and the prior packed baseline. To instrument, prefix the command with
`AIC_CLONE_PROFILE_OUTPUT=<fresh JSON path> node --import ./docs/benchmarks/constructive-factory-closure/clone-profile.mjs`
instead of just `node`. The preload is diagnostic only and writes no stdout. Before implementation is
commit `5327fda`; final implementation is this slice. Reuse identical catalogs, baseline and options,
with fresh output directories. `compare-runs.mjs --before <report> --after <report> --output <fresh JSON>`
compares either two closure reports or two packing portfolios. It fails on any semantic difference.

Evidence: `copy-timings.json`, `copy-comparison.json`, `copy-forward-comparison.json`, `copy-controls.json`
and `../constructive-cluster-packing/copy-matrix.json`. Raw candidate/validator/profiler files remain
in their referenced target directories. The selected diagram is available at
`http://100.114.24.86:8765/bound-interconnect-closure-copy-optimized.html`.

Reviewed against the accepted constructive design and AGENTS: connected private growth, runtime
catalogs, independent complete acceptance and area-first ranking remain unchanged. No production
solver fallback, fixed game canvas, weakened geometry gate or premature Rust migration is added.
The next quality slice returns to narrower structured interfaces within the large cluster, whose
long approach/permutation corridors dominate visible unused space. Retain the complete 9,856-area
incumbent. Final compactness, broader recipes, cleanup and the real web workflow remain unfinished.
