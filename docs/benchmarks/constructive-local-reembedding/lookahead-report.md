# Bounded private-seed portfolio

## Outcome and contract

The selected complete heavy-xiranite factory stays **41x52** and improves from
2,132 / 395 / 37 to **2,132 / 392 / 38**. All 59 facilities and the original graph's
recipe margins, target and real external endpoints remain independently validated.
Transport count precedes turns, so this is an improvement despite one extra turn.
Use lookahead-results.json as the experimental incumbent.

`lookahead-run.mjs` formalizes the preceding manually managed private lookahead.
It takes explicit original graph/catalog/validator paths, global baseline, schema-4
seed report, one-to-four seeds, one-to-three rounds and one-to-four workers. It checks
input and engine hashes before creating output, freshly validates the root and seeds,
deduplicates exact witness content and selects seeds by recomputed geometric score.
Only previously validated, unimproved block-transform audit records are eligible.

Every seed remains private. Independent child runners retain existing joint push,
routing, port, bridge and compaction policies, 512 shortlisted positions per round,
and four extra unimproved audits. Each owns a POSIX process group with a 180-second
timeout and one-second escalation. The driver awaits owned children even on failure.
Returned factories are freshly validated and compared with the immutable global
root, in stable seed order. Errors and merely branch-local gains cannot replace it.
Comparison records identify the directory and distinguish beating the original root
from beating the best result already examined. The contract lives in lookahead-contract.md.

This replaces manual private-seed manifests and ad hoc global-root comparisons with
one bounded workflow. It does not change production generation, the local pose schema,
route semantics, game data or the isolated exact solver. It is not exhaustive search.

## Controlled evidence

### Replay of the proven area gain

With target/block-rotation-final-single/report.json as the 2,173 / 388 / 36 global
root, and target/block-rotation-final-block/report.json as seed source, request one
seed, one worker and three rounds. Selection deduplicates the F30/F44 audit pair and
picks source stage 315, privately scored at 2,173 / 395 / 36. The branch repeats
13,588 attempts and returns 2,132 / 395 / 37. Its raw witness SHA256 is exactly the
previous 844860393fed9258422d85d8d29fd3a9f9e5d5abd4a7427a72203336e9169da7.
The driver promotes it only after fresh full validation and comparison with the
global 388-cell root. Current-code report: target/lookahead-replay-final/report.json,
67.55 seconds including driver validation; branch runtime is 67.09 seconds.

### Current-root portfolio and direct control

First collect a new block-rotation seed report from the preceding 2,132 / 395 / 37
incumbent, with 512 positions, one round and sixteen unimproved audits. This source
run makes 6,754 attempts in 22.17 seconds and directly improves to 2,132 / 393 / 38:
rotate the F48/F37 pair 180 degrees to envelope origin (31,45), then compact.
The earlier selected F47/F35 proposal is a competing candidate, not an additional
move applied to that final winner. Freeze the stronger 393-cell result as global root.

The portfolio has sixteen eligible records, eleven exact-content-unique witnesses
and five duplicates. Select four seeds, use two workers and cap each branch at three
push rounds. Final source: target/lookahead-portfolio-final/report.json.

| Source stage | Private seed score | Final private score | Attempts | Branch time |
| --- | --- | --- | ---: | ---: |
| 257 | 2,132 / 395 / 36 | Same | 4,608 | 21.68s |
| 1005 | 2,132 / 396 / 38 | 2,132 / 395 / 36 | 12,911 | 65.60s |
| 254 | 2,132 / 397 / 39 | 2,132 / 395 / 36 | 8,807 | 43.79s |
| 272 | 2,132 / 405 / 38 | 2,132 / 399 / 37 | 9,771 | 43.40s |

All four branches validate, finish without timeouts and stop on an unimproved
neighborhood. None beats the 393-cell global root; it remains selected. Total observed
portfolio wall time is 109.84 seconds. An earlier run before adding directory/root
comparison fields produces identical final witness hashes for every branch and root.
These are concurrent optimization observations, not isolated construction latencies
or a measured parallel speedup. Selection and concurrency bounds have separate tests.

The same-root direct push control performs 9,474 attempts in 40.66 seconds. Its
fixed-path phase reroutes seed-return chain 23; subsequent compaction removes another
physical cell without changing area. Stage 23 is 2,132 / 392 / 38, followed by an
unimproved round. That independently validated result is the new global incumbent.
The four private branches did **not** produce this new improvement. Their useful
evidence is safe root retention and reproducible branch outcomes.

The reports listed in lookahead-results.json have verified matching input and engine
hashes at this slice's commit e67c08d. Preliminary lookahead-replay/ and lookahead-portfolio/ reports
precede the final comparison-record addition; use their `-final` counterparts.

## Verification and visual findings

265 Node tests pass, including nine new score, exact-content deduplication, tie,
area-first acceptance, invalid-budget, bounded-worker, await-on-failure and process
tests. The latter cover spawn failure, log non-overwrite, resistant group leaders,
and resistant descendants surviving their leader's SIGTERM. cargo check and 524 Rust
tests pass, with two existing ignores. No Rust files changed.

Seven actual CLI/validation controls pass: invalid budget, stale engine, different
graph, no seeds, missing artifact, forged seed validity and forged root validity.
The forged witness substitutes an undeclared external endpoint. Its geometry alone
is insufficient: fresh original-graph validation rejects it despite the claimed
validity flag. Seed rejection keeps the root; root rejection returns an explicit
failure. Budget/provenance failures occur before output-directory creation.

The final 392-cell factory passes eight original-graph controls and 25 bridge mutation
controls; the unchanged control matches its raw witness hash. Personally inspected
the full native render: the bottom-right F48/F37 order is reversed, but central/left
voids and the lower seed-return span remain. This is not minimum-area evidence.

Browser checks reveal a separate viewer bug: after a focus or zoom changes the SVG's
aspect ratio, its resize observer redraws and resets the view. Immediate pan/zoom
assertions pass but do not prove persistence; the attempted focused capture repeats
the full view. A delayed check confirms the reset. Fix this next before claiming
stable focused visual inspection. Six viewport/theme checks and the full render pass.
The subsequent renderer-only repair and persistent browser checks are documented in
../constructive-bound-interconnect/viewport-report.md; that follow-up resolves this
issue and completes full/focused visual inspection without changing the factory.

Compared with AGENTS.md and the constructive design's Iterative Improvement contract:
every public selection is a complete validated factory and improves area/cells/turns
lexicographically; all worse intermediate states remain private. Limits apply to
these local experiments, not constructor dimensions. Further space improvement,
broader recipe validation, production cutover, cleanup and the requested web workflow
remain open.
