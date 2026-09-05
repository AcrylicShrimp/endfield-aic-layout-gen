# Q0: routed nearby placement selection

## Status and scope

Experiment complete under the [pre-implementation contract](../../todos/2026-09-05.12-nearby-placement-experiment.md).
The user explicitly prioritized initial placement after viewing the sparse complete factory;
Q1 fixed-chain route improvement and HTML optimization remain deferred.

Decision: retain `nearby_candidates=0` as the default. Nearby selection improves a complete small
cycle but does not complete Heavy Xiranite in the measured configurations. Keep it explicit opt-in
research evidence, not a newly adopted production default. No compact partial result replaces the
previous complete factory or its shared HTML.

## Algorithm and observable contract

`--nearby-candidates N` / request `nearby_candidates` accepts 0..=8, initially defaulting to zero.
Zero preserves the coarse first-feasible constructor. A positive N enumerates legal rotations and
edge/center-aligned cardinal poses around existing facilities at heuristic gaps of 1, 2 and 4 cells.
These are candidate offsets, not required game spacing or a W/H limit. Facility and existing
transport occupancy filter footprint collisions. Duplicate poses are removed before cheap ranking
by prospective bounding-box area, distance to the connected anchor, and deterministic pose order.
The shortlist contains at most 32 poses; enumeration memory is linear in current facility count.

The final implementation also protects pending new-port escape corridors in both directions:
new footprints cannot occupy existing pending corridors, and new pending corridors cannot point
into existing footprints. Edges induced by the current trial are excluded because they close now.
The coarse escape poses undergo the same check when nearby mode is enabled. The three-cell length
is shared with the router's existing reservation constant, not invented as a new game rule. This
is a conservative constructive heuristic, not proof that other legal turning routes are impossible.

Each shortlisted trial begins from the same immutable incumbent, places one frontier facility,
and immediately routes the newly induced edges using the existing pipe-first connection engine.
Independent network validation, remaining port-demand analysis, future-boundary accessibility and
original-wiring rate audit remain required. Up to N successfully validated alternatives are compared
by actual used area, unique physical transport-layer cells and the existing route-turn metric.
Equal scores retain the earlier alternative. Only the best candidate survives; no rejected candidate
is committed or used as input to another alternative. Atomic-cycle growth still commits as a group.

If the nearby shortlist has no successful candidate, the same coarse escape sequence remains
available within the remaining growth budget. This is not an exact-solver fallback. Every actual
placement trial and A* search, including discarded alternatives and failed group attempts, consumes
the existing per-growth budget. The shortlist and valid-candidate ceilings do not reset that budget.

Request schema 1 adds the optional default-zero setting. Production report schema 2 gains an
additive `placement_decisions` array; strict consumers must accept this field. Each entry identifies
the attempted facility/anchor/variant, shortlisted/attempted/valid counts, first and selected scores,
selected zero-based ordinal, padded-input pose and a stable termination category. Entries include
private trials from later-failed cycle groups: they are selection evidence, not accepted growth
history. `steps` remains the authority for committed validated states. Failure diagnostics retain
the last valid factory and exhausted bounds; no compact partial result counts as an improvement
over a complete factory. Full output remains file-backed with one short stdout summary.

## Measurement protocol

Preserve the `0046d39` release binary before editing. Finish release compilation and workspace tests
before running the [harness](run.mjs). Compare baseline, candidate zero, nearby-first (N=1), and
best-of-up-to-four (N=4) using identical external catalogs, source plans, seed, step/placement/A*
budgets and full JSON/HTML mode. Candidate-zero witnesses and search choices must match baseline;
nearby modes intentionally may change geometry, but repeated runs must remain deterministic.

```sh
node docs/benchmarks/constructive-nearby-placement/run.mjs /absolute/path/baseline-aic-cli target/release/aic-cli /absolute/path/new-output-directory
```

`AIC_PLACEMENT_ROUNDS` controls repetition (1..10); `AIC_PLACEMENT_VARIANTS` selects named variants.
The macOS harness retains full and seed-cycle successes/failures, native CPU/RSS, CLI/planner time,
scores, operation counts, selection evidence and original input/binary hashes. Large raw artifacts
stay outside git. Parsing, score calculation and hashing are outside the timed child. Do not rank
an incomplete graph's smaller area as a successful quality improvement.

## Controlled development results

The first implementation did not align footprint selection with the router's pending-port corridor
reservation. [Unguarded samples](unguarded.json) stop at 19/59 facilities for N=1 and 8/59 for N=4.
[Additional counts](counts.json) N=2 and N=8 also stop at eight. Small cycles complete in all cases.
The old binary and zero mode retain exactly the same final/intermediate witnesses and search choices.

Inspection of the failed N=4 node finds footprints at (2,1), (1,1), (2,2), (1,2), (2,3), (1,3)
in front of three pending thickener input ports. Its next 1 unit/s moss-powder connection needs
multiple lanes under the runtime belt capacity. N=1 also leaves footprints in pending corridors.
These observed collisions motivated the guard above; they do not prove that every failed route was
impossible or that this guard alone would restore completion.

The separate [budget control](budget.json) raises placements 256 -> 1,024 and route searches
4,096 -> 16,384 per growth, leaving per-connection and candidate-enumeration limits unchanged.
N=1 still stops at 19 (11.848 s planner); N=4 reaches 28 but stops after 162.870 s planner and
18,350 total A* calls. More budget can change the frontier reached, but does not establish completion.
This is not a same-budget speed comparison or evidence of global infeasibility.

## Final guarded matrix

The [guarded matrix](guarded.json) uses original budgets. Scores of partial nodes below are explicitly
not comparable to the complete baseline as factory-quality results. Times are individual diagnostic
planner samples; the repeated experiment supplies distributions separately.

| Heavy Xiranite policy | Complete | Facilities retained | Retained area / tiles / turns | Planner, s |
| --- | --- | ---: | --- | ---: |
| Baseline | Yes | 59 | 23,100 / 4,265 / 298 | 3.140 |
| Candidate zero | Yes | 59 | 23,100 / 4,265 / 298 | 3.160 |
| Nearby 1 | No | 9 | 494 / 149 / 18 | 3.118 |
| Nearby 2 | No | 22 | 1,794 / 789 / 80 | 7.751 |
| Nearby 4 | No | 27 | 1,953 / 914 / 75 | 4.333 |
| Nearby 8 | No | 29 | 2,268 / 912 / 84 | 69.644 |

N=4 selects an alternative better than the first successful proposal in 17 of 30 private facility
selection calls. It evaluates 104 valid nearby candidates over 307 nearby placement trials. Thus
the comparison is actually operating, but locally better choices do not imply a completable factory.
N=8 evaluates 260 valid nearby candidates and still fails; candidate count is not a monotonic quality
or completion knob. Counts include successful private substeps in cycle groups that later fail.

| Complete seed-cycle policy | Area | Physical tiles | Turns |
| --- | ---: | ---: | ---: |
| Baseline / zero | 294 | 40 | 2 |
| Nearby 1 | 160 | 38 | 5 |
| Nearby 2 / 4 | 182 | 24 | 2 |
| Nearby 8 | 192 | 39 | 4 |

N=1 reduces this complete small factory's area 45.6%; N=4 reduces area 38.1% and physical tiles 40%.
N=4's final result is worse in the first-priority area than N=1, despite winning its local comparison:
later cycle closure and external routing occur after selection. This is direct small-case evidence
that scoring only the current facility's routed addition is myopic. It is not a global best-of-N search.

## Next bottleneck and research decision

Guarded N=4 stops while adding an enriched-moss producer after 27 facilities. Its last geometrically
valid candidate for connection `wiring-edge:8dee9d24d17b98cf3cc8798a9c28a04499b0a77a3d7dea14045c6e199831aee7`
is rejected because it blocks pending requirement
`wiring-edge:7565bb835dbe056399c0de47fcf3bd21108488893615068a6cba7f69078572de`.
The report retains this obstruction separately from the committed node. The run records 227
future-boundary rejections overall. N=2 exposes another explicit pending-boundary obstruction;
N=1 exhausts its route budget in an atomic seed cycle. These are observed bounded planner failures,
not proofs about all positions, all routes, or all placements of the factory.

Preserving short straight corridors fixes a concrete candidate/router mismatch, but does not ensure
that all future connections can coexist. Do not remove the boundary validator or add an arbitrary
penalty that overrides area-first scoring. The next controlled question is whether reconsidering
the previous growth choice together with the newly blocked connection can cross this failure.
Replay a bounded recent growth window from its validated checkpoint and evaluate the whole connected
window before committing; retain unrelated earlier geometry. Start with the recorded obstruction,
not a full-factory optimizer or an unbounded beam. This requires its own transaction/budget contract.

The old coarse candidate sequence remains a real escape policy and the reliable default, not an
obsolete compatibility adapter. Nearby generation, selection evidence and shared corridor policy
are isolated from global exact tooling. No full report schema cutover, HTML redesign, new dependency,
global placement phase or W/H input is introduced by this experiment.

## Repetition and verification

The [three rotated repeats](repeated.json) retain every sample and reproduce identical witnesses,
scores, search choices and failure points within each mode. Zero mode also matches the preserved
baseline exactly. No compiler, test or screenshot process overlaps these timings.

| Heavy Xiranite | Planner median [range], s | CLI median [range], s | RSS median, decimal MB |
| --- | --- | --- | ---: |
| Complete baseline | 3.158 [3.135, 3.160] | 4.813 [4.787, 4.845] | 286.228 |
| Complete zero mode | 3.152 [3.149, 3.209] | 4.859 [4.822, 4.877] | 286.310 |
| Partial nearby 1, 9 facilities | 3.109 [3.109, 3.126] | 3.154 [3.154, 3.172] | 13.681 |
| Partial nearby 4, 27 facilities | 4.318 [4.315, 4.326] | 4.498 [4.495, 4.506] | 41.091 |

Partial runs have smaller output histories and stop early; their latency/RSS is not a construction
speed or memory improvement. Baseline/zero timing ranges overlap. This is a quality/failure
experiment, not a new runtime-optimization claim. Small-cycle CLI medians are 30.343/30.099/29.892/
31.274 ms for baseline/zero/1/4, respectively. Timing variability is retained in the structured data.

After timing, inspected the actual N=4 HTML: it shows a 93 x 21 strip with facilities tightly
arranged amid long shared corridors, clearly labeled failed partial history (27/59 facilities,
36/89 internal requirements). Compactness is visually real, but no complete improved factory exists
in this experiment. The existing 59-facility shared page is unchanged.

- `cargo fmt --all`, `cargo fmt --all --check`, `cargo check --workspace`: passed.
- `cargo test --workspace --quiet`: 495 passed, two existing research tests ignored. New tests
  cover repeated chain/branch/cycle growth, independent validation/coverage of every accepted step,
  strict selected-versus-first score ordering, bounded trials/routes, invalid limits, retained seed
  on exhausted cycle growth, and pending-corridor versus induced-edge distinction.
- `cargo build --release -p aic-cli`: passed before final measurements. No new dependency.
- Fifty-two release CLI executions across exploratory/count/budget/guarded/repeated matrices were
  retained, including failures. Separate manual N=9 input rejected with `production-input-invalid`,
  exit one, saved JSON and one 188-byte stdout completion line.
- Harness syntax, documentation links, binary provenance, original shared HTML hash and
  `git diff --check` verified. Raw HTML/JSON stays outside git; structured evidence is committed.
- The initial unguarded binary is preserved separately. Its records are an intermediate experiment,
  not claims about current guarded code. Reproduce the final algorithm with current source; reproducing
  that intermediate variant requires omitting the pending-corridor candidate filters described above.

Remaining risk: deterministic local choices can still trap future multi-lane or cycle connections;
no candidate ranking, finite shortlist, corridor reservation or timeout is a completeness proof.
The next slice should define and test bounded continuation/reconsideration on the saved obstruction.
