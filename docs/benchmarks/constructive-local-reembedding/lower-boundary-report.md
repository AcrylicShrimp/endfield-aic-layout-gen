# Lower-boundary routing obstruction

## Decision

Keep the validated 41x53 / 2,173 area / 407 transport-cell / 41-turn incumbent in
push-boundary-results.json. This slice produces no new complete factory. The next
placement experiment should allow relative movement within the lower column while
preserving its upstream access corridor, rather than further enlarging the same
rigid upward translation or port-only enumeration.

This is a measured neighborhood decision, not a proof that rigid translation or a
smaller factory is impossible. All failed/private and relaxed results stay separate
from the complete incumbent. Its witness bytes remain unchanged.

## Exact request and successive obstructions

Replay stage 97 of target/geometric-push-boundary/report.json, whose parent is the
408-cell incumbent of that run. It moves twelve facilities up one cell and releases
the two incident lower bridges, retaining terminal-flexible endpoints, downstream
junction ownership and an eight-new-crossing budget. Do not substitute the current
407-cell winner or chain IDs extracted without that bridge release.

The priority replay tries 32 requests: affected/belt/all rerouting in near/far order,
six distinct failure-priority states, and twenty vacant-port attempts. None produces
a candidate. Near order fails chain 61 (F26 to F18, moss powder 3); far order exposes
chain 27 (F32 to its converger, moss 3). Prioritization also exposes chain 50 (F27 to
F19, moss powder 2).

In the finite body/terminal relaxation, chains 61 and 50 need zero occupied terminal
cells. Chain 27 needs two: F44 output-belt-0 at (9,30) and F31 input-belt-0 at (11,30).
Its original horizontal route lies above F33; the upward group translation occupies
that route with F33's body. The relaxed alternative passes through the seed interface
above it. It is NOT valid wiring: component arms, ordinary wires and crossing geometry
are absent from this relaxation. Zero relaxed blockers does not imply a routable chain.

## Explicit cooperative controls

The replay tool now accepts `--chains ID[,ID...]`. It pairs each selected access result
with a retained-port request that actually fails that chain, checks common movement
and crossing policies, unions the obstruction owners and singleton facility ends,
and keeps the existing eight-terminal and four-order budgets. This removes the
first-failure-only restriction without adding separate per-chain repair paths.
Schema 2 replaces scalar `chain` with ordered `chains` and adds `control_records`.

| Selected chains | Terminals | Assignments | Route trials | Termination | Candidates |
| --- | ---: | ---: | ---: | --- | ---: |
| 27 | 3 | 80 | 320 | Domain exhausted | 0 |
| 27,61 | 5 | 640 | 2,560 | Domain exhausted | 0 |
| 27,61,50 | 7 | 4,096 | 16,384 | Candidate limit | 0 |

The single-chain interface expansion has the same three terminals as its terminal
scope control. The three-chain domain has 5,120 possible assignments before the
4,096-candidate cap; do not report it exhausted. Its failures include chains 27, 50,
51 and 61. These trials use ordinary affected-chain rerouting with near/far order,
with and without the selected priority list; they are not exhaustive path search.
Observed final batch times are 0.65s, 6.65s and 31.26s, respectively, under concurrent
test/experiment work. No constructor-latency or speedup claim follows.

Final reports: target/lower-boundary-cooperative-final-{single,joint,three}/report.json.
Their engine source hashes match the committed implementation. Reproduction:

```sh
node docs/benchmarks/constructive-local-reembedding/priority-probe.mjs \
  --report target/geometric-push-boundary/report.json --stage 97 \
  --catalog-directory data/game/normalized --output-directory NEW_PRIORITY_DIRECTORY \
  --max-new-crossings 8 --port-policy vacant
node docs/benchmarks/constructive-local-reembedding/cooperative-probe.mjs \
  --report NEW_PRIORITY_DIRECTORY/report.json --chains 27,61,50 \
  --catalog-directory data/game/normalized --scope interfaces \
  --output-directory NEW_COOPERATIVE_DIRECTORY
```

## Placement and component alternatives

A bounded exploratory replay adds the facility ends of either chain 61 or chain 50
to the original moving seed, then applies the existing contact closure. The first
adds F26 (13 members); the second adds F27 and then F26 (14 members). Recompute the
three obstruction-terminal port domains for each moved geometry and try near/far:
320 requests, zero candidates. Every 14-member request reaches the chain-27 failure;
the lower crossing conflict no longer terminates those requests first. This is
partial routing progress only. Adding F32 to the seed instead reaches the existing
16-member closure budget, not a factory impossibility result.

On the 14-member group, the existing component proposal generator supplies thirteen
converger poses. Its 26 retained-port near/far attempts still yield no complete
candidate: twenty fail chain 27, five chain 28 and one chain 31. Component relocation
can expose a later chain, but does not complete this measured neighborhood.
Exploratory scripts and full requests are retained at target/lower-boundary-repair.mjs,
target/lower-boundary-component-repair.mjs and their corresponding output reports.

## Verification and visual review

238 Node tests, cargo check and 524 Rust tests pass (two existing ignores). New tests
cover default/ordered control selection, mismatched geometry policies, retained-port
eligibility, nonfailure rejection, invalid/duplicate IDs and input immutability.
Four invalid CLI selections fail before creating output directories. No Rust or
production routing implementation changes. The existing 407-cell witness hash still
matches its independently validated snapshot; no new factory acceptance is claimed.

Personally inspected native full priority and three-chain overlays, lower crossing
focus, the chain-27 access/seed-interface focus, and the 14-member group's lower focus.
Five diagnostic pages pass six viewport/theme checks each. Dashed bodies and relaxed
paths remain labeled rejected/not valid; solid geometry is the unchanged validated
parent. Focused access viewer:
http://100.114.24.86:8765/bound-interconnect-closure-lower-boundary-access.html.

Compared against AGENTS.md and the constructive-planner design: improvements remain
private, bounded and subject to complete-factory acceptance; no global placement-only
construction, fixed production canvas, semantic relaxation or exact-solver fallback
is introduced. Non-rigid local packing is the next hypothesis, not an implemented
production cutover. Acceptable final compactness, recipe breadth, cleanup and web
delivery remain unfulfilled parts of the broader objective.
