# Envelope-support cohort experiment

## Outcome

Both contact-only and boundary-augmented push portfolios reach the identical complete
factory: 59 facilities, seven components, 41x53 / 2,173 area, 407 physical transport
cells and 41 turns. The input is push-results.json (2,173 / 408 / 40). The one-cell
wiring gain takes priority over one additional turn. Keep the contact-only result
as the selected incumbent; the added boundary seeds contribute no measured gain.
Relative to the preceding bridge-index incumbent, transport decreases by 18 cells
from 425, with no further area reduction.

| Seed portfolio | Routing attempts | Observed elapsed | Result |
| --- | ---: | ---: | --- |
| Contact control | 6,282 | 36.59s | 2,173 / 407 / 41 |
| Contact plus boundary | 6,255 | 35.82s | Identical witness |

These concurrent batch observations are not constructor latency or a speedup claim.
Both runs use joint mode, a six-round cap, 256 proposed moves per round, 4,096
sampled poses, at most 16 moved facilities, near/far routing, downstream junctions,
terminal-flexible endpoints, bounded vacant-port retries, eight new crossings,
incident bridge release, independent-layer compaction and four unimproved audits.
Both stop at an unimproved third round. Boundary adds 12 seeds initially and 11 in
each subsequent round; selected ancestors 521 and 2553 still have contact seeds.

Reports are target/geometric-push-contact-control/report.json and
target/geometric-push-boundary/report.json. All recorded source and input hashes
match current files. The selected witness byte SHA-256 is
84159e4157001d2bb1390f0a83bc593d3f6bdf0264c648e60e06b303fcfbc175 in both runs.

## New neighborhood and next obstruction

The optional `--push-seeds boundary` adds cohorts derived from actual facility body
envelope support. It does not move external terminals or infer game gaps from the
one-through-four-cell proposal bands. Each seed uses the same contact closure,
movement checks, local bridge-release budget and factory validation as the control.
The original seed and final closure remain separately observable in reports.

Boundary report stage 97 seeds seven lower-envelope facilities (maxY, band two),
grows to twelve through body contacts and attempts a one-cell upward translation.
It passes geometry screening but fails routing chain 61, item-plant-moss-powder-3,
with `local-no-path` after six expansions. This request releases the two incident
bridges; chain IDs from a replay without that release are not interchangeable.
This is a local attempted-route failure, not proof that the group or factory is
infeasible. Next replay this exact parent/request and inspect endpoint access and
route ordering before extending the neighborhood again.

## Verification and visual inspection

237 Node tests and 524 Rust tests pass (two existing ignores); cargo check passes.
Eight original-graph controls and sixteen crossing controls pass on the selected
407-cell factory. Two manual invalid-option CLI checks reject unknown seed policies
and use outside push mode. New cohort tests cover unequal body sizes, non-origin
coordinates, separated support members, deduplication, oversized seeds, determinism
and input immutability. No Rust code, dependency or production command changes.

Personally inspected both native full results and the contact control's upper-head
focus. Both pages pass six viewport/theme checks. Central lower empty space and the
long bottom return remain; the left lower bodies still hold the measured height.
Viewer: http://100.114.24.86:8765/bound-interconnect-closure-geometric-contact-control.html.

Compared with AGENTS.md and docs/designs/2026-09-02.00-constructive-layout-planner.md:
this is a bounded improvement experiment on a complete factory, not deferred global
construction. Failed private candidates cannot replace the validated incumbent;
original graph/external obligations and area-first scoring are unchanged. No global
optimality, acceptable-final-compactness or production cutover is claimed. Broader
recipe coverage, cleanup and web delivery remain open.
