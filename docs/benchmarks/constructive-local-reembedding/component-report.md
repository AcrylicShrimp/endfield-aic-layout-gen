# Component pose replanning: small wiring improvement, unchanged area

## Outcome

The full incumbent improves from **2,679 / 444 / 45** to **2,679 / 438 / 39**
(used bounding area / physical transport cells / turns). All 59 facilities, seven
components, original recipe obligations and external margins remain independently
validated. Used bounds remain **47x57**. This does not solve the central dead space.

The user annotated the previous non-improving compound alternative, not the incumbent.
Their merger/splitter alignment observation is borne out by this experiment. That
alternative improves from 2,679 / 465 / 53 to **2,679 / 459 / 44**, but still loses to
the incumbent and is not promoted. In its actual close-up, the M-to-S connection becomes
straight; the F19 input approaches still bend around the retained port assignments.

## Implementation and scope

`componentPoses` adds explicit existing-component x/y/rotation choices to the same
final-state transaction as schema-3 facility translations. Identities, types, rates,
network membership and logical branch allocation are preserved. Runtime catalog
footprints/rotations are checked; this initial family supports one-cell components.
Every incident attachment arm rotates by the pose delta, including incoming arms.
In-place rotations invalidate incident chains. Body/anchor collisions and displaced
routes use the existing transactional movement and A* repair machinery. An explicit
original pose pins a component against implicit downstream ownership translation.

The standalone `--mode component` portfolio derives positions from incident endpoint
coordinates and catalog rotations. It rejects body/anchor/terminal collisions and
body-blocked attachment arms, ranks by endpoint distance, shortlists at most 32 poses
per component and diversifies across components. Near/far restoration orders are
tested. Post-route independent-layer compaction is retained. This is a bounded proposal
family, not an exhaustive pose/route search or a minimum-area proof.

The API supports simultaneous facility/component changes, with a unit test for explicit
pose override. This milestone's CLI only proposes standalone single-component changes.
It does not yet resynthesize logical junction topology, permute individual branch-arm
assignments freely, or generate vacant input-port reassignment proposals. F19 has six
belt input slots and uses three in the annotated alternative. Existing paired-port
proposals exchange occupied same-item/equal-rate slots; they do not search vacant ones.
The lower-level port assignment API can express that next candidate family.

## Controlled experiments

All runs load the runtime catalogs, start from a complete independently validated
factory, use terminal-flexible endpoints, downstream junction ownership and independent
layer compaction. Four new bridges are permitted per repair, except the group follow-up
which permits eight. No original requirement is dropped.

| Experiment | Attempts | Final area/cells/turns | Termination |
| --- | ---: | --- | --- |
| Fixed-path retained-component control | 74 | 2679 / 444 / 45 | No improvement |
| Standalone component poses, three rounds | 1242 | 2679 / 438 / 39 | Round budget |
| Continue from improved incumbent | 410 | 2679 / 438 / 39 | No improvement |
| Component poses on annotated alternative | 1536 | 2679 / 459 / 44 | Round budget |
| Compound group moves after component improvement | 1334 | 2679 / 438 / 39 | No improvement |

The first pose run takes 46.74s; the alternative run takes 59.15s. They ran concurrently,
so these are batch elapsed times, not controlled generation throughput measurements.
The continuation takes 13.57s and the compound follow-up 17.30s. Full stage timing,
requests, affected chains, work and failure codes remain in the referenced JSON files.
No large JSON is replayed to stdout.

After verifying the explicit-original-pose pinning rule, a final-source repeat takes
47.31s and reproduces all 1,242 attempts, statuses, final score and witness hash exactly.
The independent audits and inspected render therefore match the final selected artifact.

The first run independently validates six improving transactions and eight sampled
non-improving requests. The alternative validates seven improvements relative to its
own worse baseline plus eight sampled non-improving requests. No validator rejection
occurs. The continuation fails 314 repair requests and finds no better candidate among
the remaining 96. All 1,334 group follow-up requests fail restoration under the tested
orders/ports/budgets. These `local-no-path` results do not establish global infeasibility.

## Visual inspection and next gate

Personally inspected full and close-up native renders of the improved incumbent and
the annotated alternative. The alternative's merger moves closer to F33 and its split
trunk aligns; the incumbent's splitter and crossings improve modestly. The central
void and long outer return remain plainly visible. The smaller wiring total is not an
area improvement, and moving components alone has not unlocked the group exchanges.

Next test joint facility/interface proposals and vacant compatible input ports. Keep
the current complete factory while rebuilding all affected routes; prioritize an actual
outer-bound contraction over polishing isolated bends. Pumpkin remains an optional
bounded local oracle, not an adopted global replacement. Do not port this experimental
family to Rust yet or declare the broader research/product goal complete.

## Verification and architecture review

203 Node tests pass, including catalog pose validation, identity-pose pinning, both
directed rotation arms, immutable inputs, layer preservation, collision/bound failures,
downstream override and deterministic proposals. Workspace check and 524 Rust tests
pass (two preexisting ignores). No Rust changes or release rebuild are needed.

Eight original-graph controls pass for each selected factory and sixteen bridge controls
pass for the new incumbent. Both native pages pass six viewport/theme checks and
pan/zoom/Fit. Exact witness hashes and report locations are in `component-results.json`.

Compared against AGENTS.md and `2026-09-02.00-constructive-layout-planner.md`: this is
an isolated JS improvement experiment on already complete factories. It preserves the
production constructor, independent validation, lexicographic area-first selection,
failure rollback and local-neighborhood semantics. No global exact solver, new game
data, hardcoded facility rule, public production interface or Rust cutover is introduced.

## Reproduction

Run `constructive-local-reembedding/run.mjs` with the normal graph, baseline, five catalog,
release validator and fresh output-directory paths. For the pose experiment use
`--mode component --rounds 3 --move-limit 256 --endpoint-policy terminal-flexible
--junction-policy downstream --max-new-crossings 4 --layer-policy independent
--audit-unimproved 8`. The baseline is `compound-results.json`; the diagnostic alternative
uses the validated `target/compound-alternative-baseline.json`. Continue using the
selected report as baseline and `--rounds 8`; it stops after one unimproved round.
The control substitutes `--mode fixed`. The follow-up uses `--mode move --move-family
compound --port-policy paired --max-new-crossings 8`. Every report records input and
algorithm source hashes; retained target artifacts are research outputs, not source data.
