# Direct-neighbor placement: measured initial interpretation

## Decision

Keep this opt-in experimental baseline, not the default and not a completed implementation of the
user's intended human-style placement. It makes a consequential extra assumption: every fresh
supplier's entire footprint must be upstream of every already placed consumer along the global
axis, even when the actual connected port faces north or south. User visual review rejected this
interpretation after measurement. Do not infer that direct-neighbor placement itself is ineffective.

The candidate path skips both the old all-facility nearby scorer and coarse escape poses. It ranks
direct-neighbor endpoint distances before any routing, then accepts the first valid local pose.
Ports remain assigned before placement. Trials marked accepted are local additions and may still
belong to a later rejected private group, not committed history.

## Release results

Three deterministic repeats, rotated variant execution order, no compile/test overlap. Caps remain
256 growth steps, 256 placement attempts and 4096 A* calls per transaction, 64 A* per connection,
180 seconds process wall guard. No guard fired. Smaller partial footprints are not complete quality
improvements and early failure is not acceleration.

| Workload / mode | Facilities | Area / tiles / turns | Median constructor / full CLI seconds | A* / placements |
| --- | ---: | --- | --- | --- |
| Heavy Xiranite, default parent | 59 / 59 | 23100 / 4265 / 298 | 3.140 / 4.816 | 178 / 162 |
| Heavy Xiranite, candidate options disabled | 59 / 59 | 23100 / 4265 / 298 | 3.153 / 4.835 | 178 / 162 |
| Heavy Xiranite, closed + east + direct | 18 / 59 | 912 / 105 / 11 | 1.201 / 1.263 | 388 / 405 |
| Small cycle, default parent | 2 / 2 | 294 / 40 / 2 | 0.002 / 0.030 | 3 / 1 |
| Small cycle, closed + east + direct | 2 / 2 | 126 / 24 / 2 | 0.002 / 0.029 | 3 / 1 |

The candidate with options disabled preserves the default witness. All fixed placements match the
catalog-derived orientation; closed-mode retained snapshots have zero missing internal outputs.
Trials follow nondecreasing endpoint distance and never continue after the first locally accepted
pose. Heavy Xiranite retains 48 x 19, 18 internal requirements and no completed external terminals.
The first ten additions each need one placement trial. Step 15 spends 129 trials, and the final
failed transaction spends 256 trials. The previous fixed-orientation coarse result reaches 54 / 59
in 124.047 seconds (a prior single sample, not a same-coverage or median speed comparison).

## Confirmed obstruction

`obstruction.json` contains the exact retained node and last rejected trial, plus adjacent-cell
occupancy. At unfinished input `(32,7)` of the existing Xiranite oven:

- west `(31,7)` is occupied by a mix pool;
- east `(33,7)` is the receiving oven;
- south `(32,8)` is an existing enriched-carbon belt terminal;
- north `(32,6)` is initially empty, but the rejected new Xiranite-powder belt turns there.

This closes the remaining approach. The boundary screen rejects the candidate; it is never committed.
The final transaction performs 240 A* calls, finds 238 completed lane bundles, and rejects those
bundles in the remaining-boundary checks. The last retained state has zero overlapping pending
endpoint reservations. Straight three-cell reservations extend west into occupied facility space
here; they do not protect the actual north approach. This last counterexample does not explain every
failed pose and is not proof of global infeasibility. Existing ports are not reassigned, candidate
routes are not jointly reconstructed, and prior placements are not relocated.

## User visual review: the problem starts earlier

The first committed step already has F00 at `(6,4)` with rotation 90 and F01 at `(0,0)` with
rotation 0. F01's east-facing `output-pipe-3` connects `(5,3)` to F00's north-facing
`input-pipe-5` at `(8,3)`. It is the first candidate, endpoint distance three, with no preceding
route obstruction. The rigid global upstream half-plane plus preassigned perpendicular ports
produces the diagonal placement. This is not a routing-search accident.

The same mechanism places F03 southwest of F02. In final retained coordinates, F02 is the mix pool
at `(27,3)` and F03 the oven at `(21,8)`. Material flows **F03 to F02**, through a belt from
`(26,8)` to `(28,8)`, into F02's south-facing input. Again the first proposal succeeds; the hard
left-side footprint restriction excludes positions beside the actual south input face.

The user proposed belt-before-pipe wiring. This is not implemented in this slice. The current
pipe-first frontier and route ordering, and output-first mixed-transport orientation selection,
remain unchanged. Next define separately: transport priority during graph growth, facility orientation
reference, actual connection-face placement, and routing order within a transaction. Do not silently
claim that merely reversing the router sort implements all four. The accepted default design still
says pipes first; a user-approved belt-first experiment needs an explicit scoped exception.

The user then specified a straight-row, one-empty-cell baseline for unbranched one-to-one chains.
This reverses the current premature commitment: establish facility alignment, then choose ports
compatible with that placement. Different port offsets must not automatically shift the facility.
Mixed-face fixed orientations need an explicit exception rather than claiming every pair can face
each other. The current loop does not implement this follow-up, and the observed F00/F01 pair also
has other pending input demands, so it must not be mislabeled an unbranched full-graph chain.

## Profiling

One separate profiled release run reproduces the same final node. Constructor 1.210 s; aggregate
A* 451.9 ms, boundary escape 212.3 ms, network validation 197.0 ms, node-clone exclusive 96.6 ms.
Facility-addition exclusive time is 40.6 ms, including proposal generation and other uninstrumented
addition work; there is no dedicated proposal-generation timer, so do not attribute all of it to
ranking. Inclusive parent/child times overlap and must not be summed. Four boundary workers run;
their summed time is separate from caller wall time. Report file 15.6 ms, HTML rendering 11.7 ms,
HTML writing 0.8 ms. Detailed profile is in `profile.json`, not stdout.

## Verification and reproduction

- `cargo fmt --all`; `cargo check --workspace`; `cargo test --workspace`: 509 passed, two existing
  research ignores. Four new tests cover axes, graph directions, multiple direct neighbors, exclusion
  of unrelated anchors, deterministic first-valid decisions, cycles, flags and exhausted budgets.
- Release CLI build; six initial runs plus eighteen repeated runs; one separate profiled run.
  File reports and short stdout summaries checked by the harness.
- Run `AIC_DIRECT_EXPERIMENT=1 AIC_ORDER_ROUNDS=3 node
  docs/benchmarks/constructive-downstream-order/run.mjs BASELINE_BINARY CANDIDATE_BINARY NEW_DIRECTORY`.
  Baseline is `7c0236a`. `matrix.json` and `repeated.json` include source/input/binary hashes.
- Full raw reports and HTML are local in `/tmp/aic-direct.1UUfPc/`. Recreate the diagnostic with
  `node docs/benchmarks/constructive-direct-placement/inspect.mjs REPORT NEW_EVIDENCE_JSON NEW_HTML`.
  The script asserts this regression's actual counts and adjacent occupancy before adding labels.
- Existing CLI HTML and separate before/after diagnostic served through the existing tailnet server.
  Actual full-layout and cropped screenshots inspected; diagnostic toggles and horizontal overflow
  checked at 360/736 pixels in light/dark mode (eight checks). No renderer performance work, public
  Internet exposure or exact solver changes.

This slice preserves immediate routing, independent geometry/capacity/original-wiring validation,
transactional rollback of private failed work and the accepted complete incumbent. Proposal distance
does not change area/tiles/turns priority for committed complete-layout improvements.
