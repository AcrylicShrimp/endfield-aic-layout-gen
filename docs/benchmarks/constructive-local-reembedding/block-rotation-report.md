# Rigid block rotation and bounded lookahead

## Outcome

The complete heavy-xiranite factory shrinks from **41x53 to 41x52**, reducing used
area from 2,173 to 2,132. The selected witness has 395 physical transport cells and
37 turns, compared with 395 / 38 at the start of this slice. All 59 facilities,
original recipe margins, target output and real external endpoints still validate.
Use block-rotation-results.json as the new incumbent.

The decisive improvement requires looking beyond a valid but initially worse layout.
A rigid F30/F44 block rotation fills part of the central void but alone costs extra
wiring. Keeping it as a private seed permits a later F35/F47 move and compaction to
reduce height. The old global factory remains selected until the complete lookahead
result is both independently validated and better than that global root.

This is not a minimum-area result, an exhaustive lookahead search, or a production
cutover. Central and left-side voids and the long lower return still remain.

## New proposal contract

`rigidBlockPose` rotates two to four facility rectangles around their common body
envelope, then translates the envelope to a proposed origin. Every facility receives
the same quarter-turn delta; non-square dimensions and world ports transform
coherently. Equal translation vectors share a primitive. The request stays inside
the existing schema-4, four-primitive, sixteen-facility movement contract.

Groups come from dependency pairs/cycles/serial triples and graph-connected contiguous
aligned runs. The latter discovers the F33/F46/F38 vertical triple without pretending
that this subset is an entire SCC. Candidate origins interleave currently empty
rectangle fits with nearby offsets. Optionally pair the transform with an inward
boundary closure after removing the rotated members from its uniform translation.

All actual poses pass the existing simultaneous movement gate against the original
parent. No pre-rotated invalid witness is fed to routing. Incident/body-covered bridge
release remains limited to eight; new crossings, routing budgets, fixed external
positions and the complete-factory gate are unchanged. The new runner family is
`--move-family block-rotate`, requiring move/joint mode and bridge release.

Component-pose proposals now accept the same optional facility rotation list as
port-domain proposals. Body/terminal screening and their distance proxies use the
rotated catalog shape. This adds coherent component replay support, not an automatic
new repair fallback in the main runner.

## Experiments and selection

First remeasure the new 395-cell parent. Boundary-push, rotated-deflection and compound
controls take 14,508, 6,960 and 7,779 attempts respectively. Only push improves it,
to 2,173 / 392 / 36 after three rounds. Reports are
target/rotation-parent-area-{boundary,deflect,compound}/report.json, recorded at
the preceding engine revision before this slice's new proposal module.

Matched current-source controls then start from that same 392-cell parent, with
joint mode, 512 selected positions, a three-round cap, terminal-flexible endpoints,
downstream junctions, eight new crossings, bridge release, vacant-port repair,
independent-layer compaction and eight unimproved audits.

| Family | Sampled poses | Routed positions | Attempts | Selected score | Observed elapsed |
| --- | ---: | ---: | ---: | --- | ---: |
| Rigid block rotation | 10,915 | 512 of 2,308 survivors | 7,050 | 2,173 / 392 / 36 | 30.41s |
| Boundary push | 4,096 | 512 of 1,185 survivors | 4,835 | Same witness | 23.27s |
| Singleton rotation | 3,186 over two rounds | 779 | 7,966 | 2,173 / 388 / 36 | 33.40s |

These are concurrent optimization-batch observations, not isolated speed measurements
or construction latencies. All three final reports in target/block-rotation-final-*
have matching current engine and input hashes. Singleton rotation moves F07 up one
and rotates it to zero degrees. It stops on an unimproved second round. The 388-cell
result becomes the global root used to judge the later lookahead, not the private seed.

The block family exhausts its proposal stream before the 16,384 pose cap: 100 groups,
13 boundary seeds, two retained inward closures and eight release indexes. It does
restore complete rotated factories: eight unimproved audits pass validation. For
example, audit 315 rotates F30/F44 by 180 degrees into the central space and validates
at 2,173 / 395 / 36. Three furnace pairs also validate in both orders but do not improve the root.
Do not call every unvalidated `unimproved` trial a verified factory.

### Private lookahead that reduces area

Use the validated audit-315 factory as a **private** seed, preserving the global
2,173 / 388 / 36 incumbent. block-rotation-lookahead-seed.json records both identities
and scores; the measured run used the identical manifest under target/. Run the
existing boundary-push family with a three-round cap and four unimproved audits.
No routing/ownership rule or per-attempt bound changes.

1. First round, stage 3153: move F35/F47 by (-2,+6), then existing compaction yields
   2,132 / 396 / 38. The newly available old F30/F44 region is useful here.
2. Second round, stage 4962: move F34/F24/F17 left one, yielding 2,132 / 395 / 37.
3. Third round: no improvement; terminate the bounded neighborhood.

This run takes 13,588 attempts and 60.33s. All three 4,096-pose rounds shortlist 512
positions; their geometry-survivor counts are 813, 741 and 728. Both selected results
and all four extra audits validate. The final score was explicitly compared with
the **global 388-cell root**, not just the worse private seed, and wins on area.
Eight original-graph controls and 25 crossing mutation controls then pass on this
final witness. Source report: target/block-rotation-lookahead-push/report.json.

The experiment establishes a concrete limitation of one-step monotone selection:
a valid block relocation rejected on wiring cost can expose an area-reducing next
move. It does not justify accepting arbitrary worse global incumbents. A bounded
private-seed portfolio with one final global-root gate is a useful next integration
step; this slice records one explicit lookahead control, not that general portfolio.

### Failed triple-bank alternative

Nine shortlisted F33/F46/F38 triple poses reach routing, including a 15x5 horizontal
bank inside the measured 17x7 void. They do not restore a complete factory. Central
stage 4418 first fails chain 29, from the rotated F33 output to the retained converger.
The relaxed path ignores wiring and arm constraints and is not a valid connection.
Priority/vacant-port replays of two representative poses take 31 and 32 attempts;
neither succeeds. One-junction controls take 1,152 attempts without a candidate.
The full product of 32 proposals for each of two affected structural junctions, over
nine geometries and both route orders, adds 18,432 attempts without a candidate.
These tests do not enumerate all ports, poses, topologies or route choices. Their
failure is separate from the successful F30/F44 lookahead.

## Verification, visuals and policy

256 Node tests pass. New tests check every rigidly rotated body cell and world port,
non-square catalog coherence, bad IDs/angles/origins, connected aligned groups,
combined block/boundary budgets, deterministic immutable proposals and rotation-aware
component proxies. cargo check and 524 Rust tests pass, with two existing ignores.
Two invalid CLI combinations reject before creating output directories. No Rust,
game data or production API changed.

Personally inspected the rejected triple-bank overlay, the validated private seed,
the intermediate 388-cell factory and the final 41x52 full/focused native render.
The final focus shows the relocated F44/F30 pair and its new crossing below F31.
Native pages pass six viewport/theme checks and pan/zoom/Fit. Final viewer:
http://100.114.24.86:8765/bound-interconnect-closure-block-lookahead-best.html.

Compared against AGENTS.md and the constructive design's Iterative Improvement
contract: every emitted accepted factory is complete and independently validated;
the old global layout survives private exploration; the final promotion respects
area, then transport cells, then turns. Current extent bounds only these local
experiments, not factory generation. The global exact solver is untouched. The new
rigid transform reuses one existing pose/repair contract instead of introducing a
second routing path. Recipe breadth, production integration, cleanup and web delivery
remain open, along with further space and execution-cost improvements.
