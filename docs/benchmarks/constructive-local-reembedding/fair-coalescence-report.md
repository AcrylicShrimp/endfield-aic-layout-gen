# Fair coalescence coverage and process-bank controls

## Result

The selected complete factory remains 46x54 (2,484 area), with transport cells reduced
425 -> 406 and turns unchanged at 40 by the compound group-exchange control. All 59
facilities and seven components validate. Interior space is not resolved. The goal
remains open; this slice is progress through a verified wiring improvement and a
diagnostic that changes the next experiment, not satisfactory area completion.

The old pair-major coalescence budget started 22 of 750 pair/link combinations and
sampled only F48->F36 and F38->F23. There are 70 facility-pair relationships, represented
by 76 distinct terminal pairs. Fair scheduling samples all 76 terminal pairs within
the same 250,000-pose budget; its first round samples 734 pairs and shortlists 67 links.
The remaining pairs may have no non-overlapping relative-contact variant; sampling
coverage is not proof of exhaustive spatial exploration.

The cutover replaces pair-major enumeration with interleaved pair/direction traversal,
diagonal X/Y domain traversal, bounded per-pair retention and link-diverse shortlisting.
`fair-coalescence-contract.md` specifies the limits. Semantics and acceptance gates are
unchanged. Source hashes include the new scheduling module. The former control reports
retain their recorded source versions rather than claiming current-source replay.

## Same-incumbent observations

| Proposal family | Attempts | Complete score: area / cells / turns | Elapsed seconds |
| --- | ---: | --- | ---: |
| Old coalescence, one unimproved round | 178 | 2,484 / 425 / 40 | 5.91 |
| Fair coalescence, three rounds | 3,072 | 2,484 / 424 / 37 | 214.28 |
| Dependency cones, three rounds | 1,536 | 2,484 / 416 / 37 | 97.54 |
| Compound exchange, three rounds | 1,536 | **2,484 / 406 / 40** | 54.94 |

All start from terminal-identity-results.json. Controls use 256 positions; coalescence
uses 512. Near/far repair order, incident bridge release, independent layers, flexible
terminal directions and at most eight new crossings are shared. Each run has an audit
budget of four unimproved candidates. Times include proposal generation, routing,
compaction and validation; they are optimization batches, not initial generation or a
matched-throughput benchmark. The old control stops after its first unimproved round;
fair coalescence's first round reaches 2,484 / 424 / 40, not its three-round score.

Fair rounds sample 734, 734 and 738 pairs and all 76 links. Geometry screening examines
11,552 / 11,643 / 11,378 candidates. Thus equal pose limits do not imply equal movement
screening or routing work. Three-round fair generation takes 139.57s and compaction
74.07s. Wider exploration helps coverage but is not a speedup or an area breakthrough.

## Visual inspection and next blocker

Personally inspected full native compound, cone and fair results, plus the upper-bank
rejected close-up. Compound exchange puts F37/F48 along the bottom, shortens the return
and moves two downstream chains upward, but transfers empty space rather than shrinking
the factory. Fair coalescence moves the F42 and F29 banks upward but leaves global bounds.

Stage 0 of the fair report moves the upper source bank and F28/F20/F14 down one cell.
The F42->F28 connection then loses its direct approach. Body-only relaxed routing finds
a 19-step detour for its former six-step route; actual affected-route repair fails.
Sixteen rip-up/order/paired-port trials produce no complete replay candidate. This is
not a routing impossibility proof. The original solid geometry and rejected dashed
positions are distinguished in the native overlay.

The replay also exposes `local-same-endpoint` on a retained zero-segment connection.
Chain schema 2 recognizes that valid geometry, but explicitly rerouting it reaches a
path API that rejects equal endpoints. Fix this inconsistency with a narrowly authorized
zero-length path contract and negative endpoint/arm tests before using failed repair as
evidence for a placement-oracle decision. Do not waive collision or graph validation.

## Verification

215 Node tests, cargo check --workspace and 524 Rust tests pass (two existing ignores).
Selected compound result passes eight original-graph and sixteen crossing controls.
All four native pages pass six viewport/theme checks. `git diff --check` passes.
No Rust changes or dependencies. Compared with AGENTS.md and the constructive-planner
design: only private complete-factory improvement research changes; production growth,
validator semantics and area-first acceptance are preserved. Pumpkin remains a candidate
for coordinated placement, not a production fallback or a global infeasibility oracle.
