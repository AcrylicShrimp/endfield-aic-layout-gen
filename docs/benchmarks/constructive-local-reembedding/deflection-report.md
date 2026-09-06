# Boundary deflection experiment

## Outcome

The new two-primitive neighborhood generates and screens non-rigid relative placement,
but does not yet restore a complete moved factory on this incumbent. Keep the validated
41x53 / 2,173 area / 407 transport-cell / 41-turn witness in push-boundary-results.json.
Both the deflection run and contact-push control retain the exact same witness bytes.

The final deflection run considers twelve boundary seeds and seven distinct closed
inward translations. It exhausts its declared 3,709 combined poses before the 16,384
pose cap. 142 proposals pass simultaneous geometry checks; all enter routing, including
112 empty-rectangle positions. Every proposal increases the endpoint-distance proxy.
Five immutable release-set indexes serve these screens; release transactions still
respect the eight-bridge limit. These are proposal-family counts, not all legal poses.

| Portfolio | Sampled poses | Routed positions | Total attempts | Observed elapsed | Final score |
| --- | ---: | ---: | ---: | ---: | --- |
| Boundary deflection | 3,709 | 142 | 2,046 | 6.35s | 2,173 / 407 / 41 |
| Contact push control | 4,096 | 256 | 2,013 | 11.35s | Same witness |

Both use a three-round cap, joint mode, 256-position selection limit, terminal-flexible
endpoints, downstream junctions, eight new crossings, incident bridge release, vacant
port repair and independent-layer compaction. Both stop after one unimproved round.
The pose portfolios differ; times are concurrent optimization-batch observations,
not constructor latency or a speed comparison. No deflection movement reaches the
complete-candidate audit gate; the two unvalidated unimproved records are fixed-path
trials, not successful facility moves. The control audits four unimproved movements.

Final reports: target/boundary-deflection-final/report.json and
target/boundary-deflection-final-control/report.json. All recorded engine/input hashes
match current files. The initial diagnostic run has the same requests/outcomes, but
predates the corrected void-position provenance field; do not use its zero void count.

## Concrete separation and repair controls

The operator detaches one facility from an inward contact-closed group and assigns
it a different translation, using the existing atomic multi-primitive movement gate.
For example, stage 177 shifts F49/F40/F39/F46/F38 up one while moving F33 eight cells
right into the central space. Stage 1310 uses the same inward group but moves F33
twelve right and two up. Thus the previous upper-column corridor is no longer filled
by F33, but new endpoint and junction constraints prevent the complete reroute.

The first example fails chain 28, from the retained converger to F23; its eastward
first step encounters the relocated F33 seed-input terminal at (16,32). A body/terminal
relaxation that ignores fixed component arms finds a different zero-terminal-contact
path, so that relaxation alone does not diagnose the constrained first step. The
second example reaches a different commodity obstruction, chain 66 beside F19/F13.
This is not proof that either relocated geometry is impossible.

Additional explicitly bounded controls on the initial report:

- Priority/vacant-port replays of stages 177 and 1310: 54 and 82 attempts, no candidate.
- Existing converger-pose proposals across F33 deflections: 292 near/far attempts,
  no candidate (target/deflection-junction-probe/report.json).
- All four F33 terminal ports jointly reassigned at stages 177, 564 and 1310:
  400 assignments per geometry, 2,400 near/far attempts, no candidate
  (target/deflection-port-probe/report.json).

These are separate controls, not an exhaustive product of junction, port, placement
and route choices. They preserve rotations. The next useful hypothesis is to admit
catalog-legal facility rotation in the local pose contract, so connection sides can
change together with relative position. That needs an explicit geometry/terminal
cutover and validation tests; it must not be simulated by feeding an invalid parent
to the existing router. No rotation support or production cutover is claimed here.

## Verification and policy comparison

241 Node tests, cargo check and 524 Rust tests pass (two existing ignores). New tests
check simultaneous disjoint primitives, actual body collisions/bounds, independent
member motion, void provenance, deterministic sampling, immutable input, invalid
budgets and rejection of separated non-detachable shared-terminal owners. Two manual
invalid CLI combinations reject before output creation. Both runner incumbents pass
independent whole-graph validation and match the selected witness hash; no new factory
is promoted merely because it passes body screening.

Personally inspected the native full stage-177 overlay and focused stage-177/stage-1310
overlays. Both pages pass six viewport/theme checks. They show rejected proposed bodies
over unchanged valid geometry; relaxed paths are explicitly NOT valid wiring. Viewer:
http://100.114.24.86:8765/bound-interconnect-closure-deflection-near.html.

Compared with AGENTS.md and the constructive-planner design's iterative-improvement
contract: the two translations are a private mutation of a complete factory; old valid
regions are retained unless the full repair validates and improves actual area-first
score. Fixed current extent defines only this neighborhood, not a production ceiling.
No game data, routing semantics, resource limit or original graph margin is relaxed.
The broader compactness, recipe breadth, cleanup and web-delivery goals remain open.
