# Declarative edit actions: bounded random comparison

Status: complete. This is the main registered learning-free experiment, preceded
by contract probes. No policy was trained and no production or training API was
replaced. The new isolated native session implements joint facility position,
rotation and physical port selection, plus row/column insertion and deletion.
Routing, complete validation and retaining an improved best are automatic.

The common-action control preserves the previous editor's results, and added
grid actions expose concrete improvement opportunities. The small sequential
sample does not establish the improvement probability on larger factories or
whether draft damage reduces it. These results verify the action contract and
show that useful actions exist; they do not establish learnability.

## Design and inputs

The [registered design](../designs/2026-09-13-declarative-edit-action-experiment.md)
and [external configuration](../../tools/factory-edit-pilot/declarative-action-experiment.json)
fix the distributions and limits. The panel contains the first 16 distinct fresh
starts admitted by both editors, with independently drawn products, rates and
randomized complete geometry. Every arm uses the same actual request and initial
witness for each factory. Existing fixed fixtures are used only in contract probes.

Generation issued 24 draws in three waves of eight: 16 selected, one constructive
failure, and seven surplus admitted starts excluded by ordered panel selection.
All selected starts changed physical geometry. Coverage is five copper-liquid,
six watered-grass, three food and two medical factories: eleven with 2–3 facilities
and five with 16–23. There were no duplicate or editor-admission failures.
Seed 602100014 requested food at 1/26,000 ms, requiring 26 facilities; construction
exhausted its routing/growth allowance. This is a bounded planner failure, not an
infeasibility conclusion. Selection did not inspect editing success.

Each sequential arm runs four streams per factory and 64 decisions per stream:
64 episodes and 4,096 decisions per arm. Five independent action families also
receive 32 one-action proposals per factory, each starting from the original
layout: 512 proposals per family. Total: 24,576 sequential decisions plus 2,560
independent proposals = 27,136. The 384 sequential episodes share only 16 factory
inputs; correlated actions are not independent factory samples.

Spatial draws are paired across the common, rotation and port arms: a facility,
an axis and a displacement in {-2,-1,1,2}, or another facility's origin with
probability 0.25. Separate draws add catalog rotations and complete compatible,
injective physical port assignments. Grid actions select joint placement with
probability 0.75 and otherwise equally select insertion/deletion, with one or two
bands. Band coordinates are sampled within the observed extent. Syntactically
invalid and physically unfinished edits count in the denominator. These rates
are properties of this sampling distribution, not of every possible policy.

The scripted old arm makes pose, repair-all and commit attempts for each placement
decision. The old random arm samples its five primitive kinds uniformly, so it
spends fewer decisions changing geometry. Only the scripted/common pair isolates
automatic orchestration at matched geometry decisions. Rotation/port/grid arms
add capabilities and change the action distribution; their differences are
descriptive rather than a clean estimate of each capability's causal effect.

## Sequential results

Area gain is `(initial area - best area) / initial area`; the mean includes every
episode, including zeros. A score improvement may only improve tiles or turns.

| Arm | Area improved / 64 | Score improved / 64 | Mean area reduction | Editing transition time, summed |
| --- | ---: | ---: | ---: | ---: |
| Old random primitives | 6 | 10 | 0.533% | 3.763 s |
| Old scripted pose → repair-all → commit | 11 | 13 | 1.897% | 6.501 s |
| Automatic common placement | 11 | 13 | 1.897% | 4.881 s |
| Automatic placement + rotation | 9 | 11 | 1.974% | 5.310 s |
| Automatic joint placement + rotation + ports | 12 | 16 | 2.700% | 5.402 s |
| Automatic joint placement + bands | 16 | 18 | 4.041% | 5.198 s |

All 64 scripted/common pairs agree on their 64 selected decisions, score sequence
and final geometry. Automatic common makes 4,096 native attempts versus 12,288,
uses 1,637,161 versus 1,762,627 charged routing expansions, 6,611 versus 7,432 route
calls, and 158 versus 191 full validations. Identical edits skip redundant routing
and validation. The total transition time falls 24.9%, including Python
observation, proposal construction, JSON boundaries and native execution. Native
execution alone rises from 1.666 to 1.751 s (5.1%); this is not a native-speedup
claim. Source hashing and independent topology audits are outside these timings.

At a per-episode 10,000-expansion prefix, mean reductions are respectively
0.533%, 1.441%, 1.882%, 0.789%, 1.382% and 1.572%. At 50,000 expansions, each arm
already retains its full measured area gain. At a 10 ms transition-time prefix,
means are 0.334%, 1.441%, 1.441%, 1.041%, 1.761% and 2.406%; at 50 ms each retains
its full measured gain. These are conservative postprocessed prefixes, excluding
an entire action that crosses the budget, not new adaptive runs or reallocations.
The full report retains decision prefixes 1, 8, 16, 32, 64 and all registered work
and time prefixes. Wall-time observations are one bounded measurement, not a
repeated performance benchmark.

Every observed sequential area gain comes from the eleven small factories; each
arm records zero in 20 episodes on five larger factories. This small, correlated
sample provides little evidence about rare improvement probabilities. It does
not establish that sequential editing is ineffective on large factories. In the
grid arm those 20 drafts first become pending or overlapping at decision 1–4.
Fourteen start with placement, three insertion and three deletion. Early draft
damage is a descriptive observation only: the experiment did not measure its
effect on later improvement or distinguish that effect from limited sampling.
The validated best remains safe even when the final private draft is unfinished.

## Independent action opportunities

Each row below starts from the same original inputs, with 512 proposals. These
are immediate area improvements, without accumulated draft damage.

| Family | Area improvements | Rate | Score improvements |
| --- | ---: | ---: | ---: |
| Common placement | 46 | 8.98% | 73 |
| Placement + rotation | 27 | 5.27% | 48 |
| Placement + rotation + ports | 38 | 7.42% | 67 |
| Band insertion | 0 | 0% | 0 |
| Band deletion | 158 | 30.86% | 158 |

Deletion rejects 329 proposals (284 facility cuts, 19 anchor removals, 26 component
cuts), leaves 13 pending and completely validates 170, including 158 area gains.
Insertion rejects 204 facility cuts, leaves five pending and validates 303, with
no immediate improvement. Adding random rotation/ports does not automatically
increase the chance of improving one move.

On the five large factories alone, deletion improves 45/160 original-start
proposals (28.13%); common placement improves 3/160, rotation 2/160 and ports 2/160.
These samples establish the existence of useful initial-state edits. They do
not determine the probability of success from later draft states. The independent
and sequential results use different action mixtures and starting states, so
their contrast cannot attribute the sequential zeros to draft damage or poor
recovery. The 160 proposals also share only five factory inputs.

The user correctly challenged an initial interpretation that emphasized the
sequential zeros as a large-factory limitation. That interpretation is withdrawn.
A separate, adequately sampled comparison of original and edited states under
the same subsequent action distribution would be needed to measure whether
improvement/recovery probabilities change. No such experiment was run here.

Insertion is a preparation operation. A separate controlled four-action probe
shows why zero immediate reward does not justify removing it: moving A1 directly
to (2,14), rotation 90, overlaps B. Insert three rows at y=10 first, and the same
move validates without overlap or pending routes. Move A1 back to (2,7), then
delete those three rows; all four complete states pass the independent directed
incidence audit. Final score improves from [700,129,18] to [700,125,16], preserving
area and reducing tiles/turns. This fixture demonstration is not counted as a
random-panel success.

## Contract verification and continuity closure

| Preserved requirement | Actual evidence |
| --- | --- |
| Fresh complete inputs and binding | Actual generation/admission CLI, all draw records and hashes for request, randomized initial and original request; same panel across all arms |
| Construct first, improve second; no exact fallback | Existing constructive generator produced the panel; all compared starts admitted by both native editors; public full-factory admission checked probe exports and sequential final witnesses |
| Directed topology, identities, rates and obligations | Immutable blueprint equality plus independent directed-chain audit on probe exports, sequential finals and selected independent witnesses; branch/merge/bridge fixture passes |
| Joint port/pose contract | Complete compatible port map applied atomically; port reuse/incompatible mapping rejected; successful C1 rotation 180 and three selected ports yields [665,139,23] from [700,129,18] |
| Port-adjacent turns and component directions | Retained immediate-port-turn fixture passes; all common controls agree; independent audit verifies component identity/channels |
| Unfinished multi-action drafts and best safety | Overlap → checkpoint → restore → clearance passes; interrupted and exhausted-route drafts return retained valid best |
| Area-first scoring and full acceptance | Joint port example accepts smaller area despite more tiles/turns; valid worse inserted draft retained privately; complete native validation can still reject proposals without overlap/pending |
| Restoration and input identity | Exact draft/ports/grid/best restoration, elapsed allowance preserved; forged input and stale revisions rejected; ported validated witness agrees after restoration |
| Resource boundaries | Unit probe exhausts route allowance; 40 no-ops skip hidden work; separate deadline/closed-session probes preserve state, best and counters before and after restore |
| Normal-path performance and isolation | Separate commands/processes only, no learner hooks; aggregate/native timings above; isolated workers reaped and no experiment workers remain |
| Existing implementation/evidence | Old native API remains the explicit comparator; predecessor tests pass; no old policies/checkpoints are silently adapted |

The new representation removes learned repair, repair-all, commit and restore-best
choices from this experimental policy surface. It directly owns draft geometry
and port assignments, sharing compatible geometry/topology helpers. Old and new
checkpoint schemas are explicitly incompatible. This is an isolated experiment,
not the authorized completion of a production cutover.

Verification passed: `cargo fmt --all`, `cargo check --workspace`, and
`cargo test --workspace` (570 passed, five ignored); two targeted Python tests;
Python compilation and actual CLI help/probe/prepare/compare commands. The seven
final CLI contract scenarios pass in 0.081 s. The supplemental preparation and
resource-boundary probes also pass. No custom propagator was introduced.

## Limits, execution and evidence

Release execution used Linux x86_64, Rust stable 1.98.1 and the existing Python
3.12 environment. At most eight workers ran on distinct CPU cores with one CPU
thread each. Per edit, routing is limited to 50,000 expansions including
preflight work; each 64-decision episode has 30 seconds and a 3,200,000-expansion
ceiling. The total ceiling was set before random comparisons to 64 × 50,000 so
old immutable configuration and automatic common have equal action allowances.
Per-worker limits are 300 seconds and 4 GiB RSS; command wall limit is 1,800
seconds, generation allowance 180 seconds per draw, and decision cap 30,000.
RSS observation covers the Python worker; descendant constructor processes are
bounded separately by the existing admission timeout and group termination.
No claim of an aggregate descendant RSS cap is made.

Panel preparation took 5.745 s. Final comparison took 12.681 s excluding parent
startup, 12.726 s at the command boundary. All 64-decision trajectories completed
without a guard termination. One-off independent audits and input/build identity
checks ran only at command/admission/export boundaries, not per action. No active
training process was instrumented or used by this experiment.

After the first matrix, review found a duplicate current-node terminal DTO whose
port could be current while its position remained initial. The unused DTO was
removed; current node positions and `port_assignments` remain authoritative and
original terminal metadata stays in the immutable blueprint. After rebuilding
and rerunning required checks/probes, the same frozen panel and action streams
were replayed once. All 27,136 decisions have identical semantic outcomes in the
final run. Timings, source references and binary identity are excluded from that
comparison. The final `comparison-v2` results above supersede `comparison-v1`;
old source/binary and both records remain available for attribution.

[Compact provenance](2026-09-13-declarative-edit-action-provenance.json) binds the
configuration, final report, actual inputs, all sixteen case reports, final
contract gates, supplementary evidence, runtime catalogs and selected source
hashes. Full local evidence is under ignored
`artifacts/factory-edit-pilot/declarative-action-20260913/declarative-action/`;
the isolated server copy is under
`/home/ashrimp/Srv/aic-declarative-action-screen-20260913/artifacts/declarative-action/`.
These are local evidence locations, not durable backup guarantees. Generated
construction HTML (24 files, 17,725,897 bytes) was removed after retention records
were written; actual generated requests, starts, checkpoints and results remain.
No raw trace or generated view is added to Git. The remote build manifest also
records macOS archive metadata sidecars; the 216 actual source bindings match the
local files. Sidecars do not participate in Rust compilation.

## Reproduction

Use a fresh isolated Linux checkout with eight available cores and the existing
project Python environment (including its established Torch dependency). No new
dependency is required. Set `PY` to that interpreter. From the repository root:

```sh
PYO3_PYTHON="$PY" cargo build --release -p aic-cli -p aic-pilot-native --features extension-module
STUDY=artifacts/declarative-action-reproduction
SPEC=tools/factory-edit-pilot/declarative-action-experiment.json
LIB=target/release/libaic_pilot_native.so
"$PY" tools/factory-edit-pilot/declarative_action_experiment.py probe --spec "$SPEC" --native-library "$LIB" --output "$STUDY/probe"
"$PY" tools/factory-edit-pilot/declarative_action_experiment.py prepare --spec "$SPEC" --native-library "$LIB" --probe-report "$STUDY/probe/report.json" --output "$STUDY/panel"
"$PY" tools/factory-edit-pilot/declarative_action_experiment.py compare --spec "$SPEC" --native-library "$LIB" --probe-report "$STUDY/probe/report.json" --panel "$STUDY/panel/panel.json" --output "$STUDY/comparison"
```

For an exact input replay, use retained actual panel inputs, preserving or
explicitly rebinding their file locations and hashes, instead of regenerating.
A seed alone is not the identity of an interrupted or regenerated input stream.
All learning and any future production cutover remain outside this completed
experiment. Large-factory success probabilities and the effect of accumulated
draft edits remain unmeasured; this small screen should not decide those questions.
