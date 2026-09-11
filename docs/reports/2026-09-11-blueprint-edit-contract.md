# Blueprint-preserving multi-action editing: first contract experiment

The new native editing contract passed its first scripted experiment. A private
draft can retain overlapping facilities and disconnected physical paths across
actions, then restore exactly the original directed blueprint. A separate
compaction case reduced validated area by 15%. These actions were specified by
the experiment, not selected by a trained policy. Long-horizon learning remains
the next research question.

## What changed

`BlueprintEditSession` is a separate native environment under `aic-pilot-native`.
It has an immutable blueprint, an editable private draft, and a retained validated
best. Moving a facility can detach incident or obstructed routes without deleting
their logical edges. The route engine repairs a selected original edge; it cannot
choose a different consumer or redefine a network. Native path preflight rejects
wrong endpoints, changed port/component channel directions and unintended
intersections before installing geometry. Explicit unknown topology fields are
schema errors.

Only a complete draft can pass commit validation. The final witness is rebuilt
from frozen metadata and checked paths, then independently validated by the
existing full physical factory validator. A lexicographically better result
replaces best; a valid but worse draft may remain available for exploration.
An interrupted or exhausted draft returns the retained valid best.

This makes the experiment's internal contract simpler: every connection has one
immutable identity and an optional physical path. Recovery cannot silently
reinterpret a missing connection. The previous full-factory PPO environment and
its checkpoint schema remain separate experiments; no compatibility adapter was
added. Production construction still grows and validates connected subgraphs.

## Controlled fixture and results

The input is the already registered external source fixture
`docs/benchmarks/constructive-bound-interconnect/fixtures/crossing-witness.json`.
Catalog data is loaded from the five normalized game-data files on execution.
The fixture has five facilities, five networks, 15 directed chains, two splitters,
one converger and one bridge. Original network, requirement, terminal, facility,
port, recipe, item and exact-rate identities are frozen. Components and external
anchors stay fixed in this first slice.

| Check | Evidence |
| --- | --- |
| Retain overlap | A0 moves onto A1; both entities remain, with 9 overlapping body cells and four pending edges |
| Reject unfinished commit | Commit is rejected without changing the overlap, revision or best |
| Multi-action recovery | Move A1 away, reposition A0, then repair all four pending edges using 417 A* expansions |
| Exact original topology | All 15 original chains, terminal metadata, branch/merge incidence and bridge directions survive the recovered witness |
| Preserve best during exploration | Recovered score is `[700, 139, 22]`; best stays at the better initial `[700, 129, 18]` |
| Reject topology edits | Wrong path endpoint, rewiring action, recipe mutation and stale revision are rejected before draft mutation |
| Interrupted draft | Failed repair leaves four pending edges; finish still returns an independently valid initial best |
| No hidden action-32 terminal | One session retains overlap through 40 actions and restores best on action 41 |
| Validated improvement | Separate C1 relocation and repair produce `[595, 130, 22]`, from `[700, 129, 18]` |
| Reward correctness | Pose and repair earn zero; the improving commit earns `0.15`; repeated commit earns zero |

Scores are `[used bounding-box area, transport tile count, route turns]`. The
compaction takes precedence despite one additional transport tile and four more
turns because area is the primary objective. It uses 820 routing expansions.
This is a separate scenario from the overlap recovery; the 15% reduction is not
evidence that overlap was necessary for this particular improvement.

The five CLI scenarios executed 57 attempts. Every final best passed a separate
public full-factory admission. All three successful commit observations also
passed an independent Python directed-chain audit of the actual exported witness,
including a repeated commit. The audit traverses physical segments from the
original source to target, checks component directions and exact rates, rejects
intermediate foreign anchors and new branches/merges, and accounts for all
physical segments and cells. It does not accept aggregate material balance as
proof of topology preservation.

## Why the topology check is stronger

An adversarial regression swaps the physical positions of same-recipe, same-rate
facilities A0 and A1, then exchanges their terminal endpoint/node assignments.
The existing per-facility material obligations and physical geometry still pass
the ordinary complete-factory validator. The new blueprint audit rejects the
changed original terminal assignments. Native draft actions cannot express this
rewiring; adding a target field to `route` is rejected without changing revision.

Additional native guards cover original bridge directions, unrelated route and
anchor occupancy, shared routing-work exhaustion, action and elapsed limits,
foreign blueprint hashes, stale revisions, and unvalidated witness exports.

## Blueprint-aware model input

`blueprint-edit-graph-v1` preserves separate static blueprint and current-state
tensors. The graph contains original directed connections, explicit facility-to-
port links, pending flags, exact-rate-derived features, all retained path-cell
coordinates and ownership, overlapping facility identities, and best-score and
resource context. A new width-40 graph actor/critic processes two message rounds.
Both actor and critic use the blueprint; neither is an adapted old PPO checkpoint.

The overlap observation produces 27 graph nodes, 27 graph edges (15 original
chains plus 12 facility/port links), 121 retained path-cell entities and 73
experimental candidate actions. Both overlapping facilities remain separate
entities at the same coordinates. Forward outputs and every parameter gradient
are finite. Altering only the model-input copy of original incidence changes
both actor and critic predictions. Regression checks also establish that retained
path geometry and the best validated record affect the model inputs/outputs.

These are input/dataflow checks with fresh random weights, not a training result.
No PPO run was launched and no existing learning-rate decision was revisited.

## Limits and next milestone

The first experiment checks one belt fixture with branching, merging and a legal
bridge. It does not establish broad admission coverage, pipe-case performance,
generalization or a policy's ability to discover useful rearrangements. V1 fixes
component/external poses and exact port assignments; unsupported ambiguous
terminal/component anchors and unanchored/self-returning chains fail admission.
Local A* has explicit work and area guards and adds no new crossings. A repair
failure is a local diagnostic, not proof of global infeasibility.

The environment can continue beyond 32 actions, but the old finite-horizon PPO
collector is not wired to it. The next milestone is a learner with persistent
drafts across rollout chunks, nonterminal value bootstrapping, and starts from
already improved/partially rearranged layouts. Measure completed exact recoveries,
validated improvement against elapsed inference time, and unfinished-draft rates.
The current commit-only area reward may be sparse; shaping must be designed and
tested explicitly rather than granting area credit to overlapping drafts.

## Reproduction and verification

From the repository root, after syncing the existing Python project:

```sh
PYO3_PYTHON="$PWD/tools/factory-edit-pilot/.venv/bin/python" cargo build --release -p aic-pilot-native --features extension-module
export AIC_NATIVE_LIBRARY="$PWD/target/release/libaic_pilot_native.dylib"
tools/factory-edit-pilot/.venv/bin/python tools/factory-edit-pilot/probe_blueprint_edit.py --spec tools/factory-edit-pilot/blueprint-edit-probe.json --native-library "$AIC_NATIVE_LIBRARY" --output target/blueprint-edit-probe
tools/factory-edit-pilot/.venv/bin/python tools/factory-edit-pilot/plot_blueprint_edit.py --probe target/blueprint-edit-probe --output target/blueprint-edit-probe/contract.png
cargo test -p aic-pilot-native full_factory::blueprint
tools/factory-edit-pilot/.venv/bin/python -m unittest discover -s tools/factory-edit-pilot -p test_blueprint_edit_contract.py -v
```

Use `.so` instead of `.dylib` on Linux. The CLI verifies the native build's source
hashes and requires a fresh output directory. Probe actions and budgets are in
`tools/factory-edit-pilot/blueprint-edit-probe.json`; no generated fixture is added.

Verification passed: nine focused native tests; three new Python behavior tests;
`cargo fmt --all`, `cargo check --workspace`, and `cargo test --workspace`
(561 passed, five existing ignored); and all 376 discovered Python tests
(369 passed, seven platform/optional skips). CLI execution and figure rendering
completed, and the figure was visually inspected. Artifact policy and whitespace
checks passed before commit.

Selected local evidence is retained under ignored
`artifacts/factory-edit-pilot/blueprint-edit-20260911/`, including the report,
scripted traces, validated witnesses, overlap observation and `contract.png`.
The [compact provenance](2026-09-11-blueprint-edit-provenance.json) records hashes;
these local artifacts are regenerable and are not backups. The experiment was
checked against the constructive planner design and the explicit draft exception
recorded in `AGENTS.md`.
