# Milestone 3: composed and externally closed factories

Status: complete for the [factory contract](factory-contract.md), with the conditional success
and unsupported cases below. The full goal remains active: remaining-body fit, completion-aware
evaluation, bounded rollback of committed chains and equal-total-work optimization are not yet
implemented. Production construction, global exact research and the Rust probe are unchanged.

## Generated scope and acceptance

Four geometry-free requests cover battery (eight facilities), citron powder (four), observed
enriched iron (nine), and the distinct eight-facility iron decomposition. All game recipes,
footprints, rotations, ports and transport capacities are loaded from external catalogs. Neither
screenshots nor absolute reference poses enter generation. The supplied boundary obligations
include every raw input and final export; power and startup/seed inventory are excluded.

The component-tree traversal derives these orders, rather than accepting a supplied placement:

- Battery: P → C1 → F1 → C2 → F2 → G1 → G2 → G3.
- Citron: G → P2 → S → P1, with the S/P1 cycle private until closed.
- Nine-facility iron: T → GP → P2 → S → P1 → G1 → F1 → G2 → F2.
- Eight-facility iron: T → GP → P → S → G1 → F1 → G2 → F2.

Each introduced facility routes all newly induced internal requirements before validation and
further growth. A singleton or reciprocal pair commits only with no internal output to an
uncovered consumer. Failure retains older committed geometry. Port selection follows actual
edge positions; body-edge-derived tangential slides supplement relative straight, 90-degree
bend and 180-degree fold proposals. The battery completion mixes right bends and straight
connections, gaps 0/1/3, and a nonzero slide for G3. It is not a rigid repeated-bend macro.

External closure runs privately after all internal requirements are routed. Each obligation
becomes exact capacity lanes with explicit external identities. Citron exports 1 powder/s as
two 0.5/s lanes with distinct facility ports and terminal pairs. Battery has five raw-input
lanes and one 0.1/s export lane; both iron requests have two 0.5/s raw-input lanes and one 0.5/s
export lane. Nine-facility iron retains two planters at utilization 1/3; eight-facility iron has
one planter at 2/3. These inventories are never substituted when validating or scoring.

Boundary trials derive a perimeter from the actual internal footprint and an explicit margin.
All lanes stay inside that trial rectangle, and every external endpoint must also be on the
appropriate extreme of the final **used** geometry. Unused perimeter capacity is not scored.
This is a defined abstract external interface, not a claim to a particular warehouse layout.

The unchanged Rust probe checks geometry and recipe flow, but deliberately keeps its own
`factory_validated: false`: it does not know the original request. The separate factory checker
verifies original inventory/utilization, exact induced internal requirements, external identities
and per-lane rates, and zero pending recipe margins. Combined acceptance requires both gates.
`verify-factory.mjs` reads an arbitrary witness and the original request without importing the
constructor or trusting its report/history. All four selected artifacts pass that separate CLI.

## Same-cap first-completion matrix

Every run has 1,024 proposals, 200,000 combined A*/access-flood expansions, 512 validations and
a 30,000 ms elapsed cap. The control runner changes only the first relative family. Each child
has its own cap and stops on its first completion; these are **not** equal-consumed-work or
one-shared-budget portfolio results. Runtime preparation/rendering are outside construction time.

| Request | First family | Complete | Used size | Area / cells / segment turns | Proposals / expansions / validations | Failure |
| --- | --- | --- | --- | --- | --- | --- |
| Battery 8 | Straight | No | — | — | 1009 / 200000 / 5 | 5 retained; expansion cap |
| Battery 8 | Left | No | — | — | 1024 / 11783 / 7 | 7 retained; proposal cap |
| Battery 8 | Right | Yes | 12 × 20 | 240 / 33 / 3 | 684 / 1004 / 15 | — |
| Battery 8 | Fold | No | — | — | 1024 / 16685 / 7 | 7 retained; proposal cap |
| Citron 4 | Straight | Yes | 23 × 10 | 230 / 24 / 2 | 71 / 383 / 7 | — |
| Citron 4 | Left | Yes | 16 × 14 | 224 / 95 / 9 | 19 / 1875 / 7 | — |
| Citron 4 | Right | Yes | 16 × 14 | 224 / 98 / 9 | 16 / 1086 / 7 | — |
| Citron 4 | Fold | Yes | 8 × 19 | 152 / 20 / 0 | 7 / 212 / 7 | — |
| Iron 9 | Straight | Yes | 28 × 17 | 476 / 36 / 3 | 254 / 2082 / 15 | — |
| Iron 9 | Left | No | — | — | 1024 / 119648 / 4 | 3 retained, 4 private; proposal cap |
| Iron 9 | Right | No | — | — | 1024 / 117699 / 4 | 3 retained, 4 private; proposal cap |
| Iron 9 | Fold | Yes | 11 × 31 | 341 / 48 / 2 | 573 / 9622 / 13 | — |
| Iron 8 | Straight | Yes | 22 × 15 | 330 / 35 / 3 | 228 / 1278 / 14 | — |
| Iron 8 | Left | Yes | 17 × 20 | 340 / 61 / 5 | 355 / 3536 / 12 | — |
| Iron 8 | Right | Yes | 17 × 18 | 306 / 56 / 5 | 343 / 2253 / 12 | — |
| Iron 8 | Fold | Yes | 11 × 26 | 286 / 44 / 2 | 572 / 8252 / 12 | — |

Eleven of sixteen controls complete. In particular, the default straight-first battery request
still fails; the runner does not silently replace it with the successful right-first policy.
The selected lowest-area results among these four controls are right for battery and fold for
the other three requests. This selection is descriptive, not a new production dispatch rule.
Segment turns exclude port-arm bends, as in milestones 1–2. Zero does not mean no directional
change. Unify that metric before the later comparison with port-arm-inclusive human references.

## Failure-led experiments without increased budgets

The first run failed all four requests after two committed facilities. Short local attachment
was not enough: early detours consumed ports needed by future suppliers or external inputs.
The first hypothesis added a same-layer flood check for enough unused, outside-reachable ports
to cover original pending lane demands. Flood expansions use the existing routing budget.
Citron then completed at area 286; iron reached all internal facilities but failed boundary
closure; battery still stopped after five facilities. This identified a proposal/access issue,
not infeasibility of the recipes.

The next hypothesis changed enumeration, trying zero slide across all gap/port alternatives
before nonzero slides. Previously, an early slide could commit an obstructing long route before
a later aligned port was even tried. At unchanged caps, straight-first citron improved to area
230 and both iron inventories completed at areas 476 and 330. Battery still failed at G1.

The battery counterexample had three pending input ports individually reachable from outside,
but only one had a clear three-cell outward stub. Their routes competed for one narrow entrance.
The next hypothesis conservatively counted a port only if its free outward stub length equaled
the remaining lane count for that facility/direction. With this rule, right-first battery
completed. Temporarily removing only that stub predicate and rerunning the four battery family
priorities produced zero completions: straight retained five facilities and left/right/fold six.
The predicate was then restored and the final full matrix reproduced the results above.

The executable counterexample is the test named `the narrow battery channel has reachable ports
but fails the multi-lane clearance heuristic`. Its fixture is independently geometry-valid and
demonstrates reachability 3 versus clearance 1. The experimental ablation changes the count in
`factory-access.mjs` from `p.reachable && p.corridor_clear` to `p.reachable` only. Historical
artifacts preserve the corresponding implementation hashes; no permanent alternate runtime
mode was added solely to retain a one-line experiment.

Neither access test is a sound game constraint. A legal future facility could occupy an enclosed
pocket; a legal route could bend sooner than the demanded stub. Conversely, clear independent
stubs do not prove simultaneous future routing or body fit. These are conservative proposal
heuristics, not reserved masks, disjoint-path proofs, a body-packing oracle, or global pruning.
The observed causal claim is limited to these fixed controls. No larger search budget, frozen
reference-placement fallback, normal-direction reservation algorithm or exact solver was added.

## Reproduction and artifacts

Build the independent probe, then run one request or its four priority controls from the root:

```sh
cargo build --release -p aic-cli --bin aic-witness-probe
node docs/benchmarks/chain-prototype/run-factory-controls.mjs \
  --request docs/benchmarks/chain-prototype/battery-factory.json \
  --recipes data/game/normalized/recipes.json \
  --facilities data/game/normalized/facilities.json \
  --items data/game/normalized/items.json \
  --transports data/game/normalized/transports.json \
  --components data/game/normalized/logistics-components.json \
  --validator target/release/aic-witness-probe \
  --output-directory target/chain-m3-battery-controls-replay
```

Use `run-factory.mjs` with the same flags for a single first-completion run. Exit 0 means combined
factory acceptance, 2 means construction stopped, and 1 means malformed input or CLI failure.
For each other inventory replace `battery` with `citron`, `iron-nine`, or `iron-eight`. Every
output directory must be fresh. The control runner writes the explicitly reordered requests
beside their runs and returns a matrix even when individual constructors stop with exit 2.

Verify a selected witness against the **original**, not reordered, graph:

```sh
node docs/benchmarks/chain-prototype/verify-factory.mjs \
  --request docs/benchmarks/chain-prototype/battery-factory.json \
  --witness target/chain-m3-battery-controls-replay/right/run/factory-witness.json \
  --recipes data/game/normalized/recipes.json \
  --facilities data/game/normalized/facilities.json \
  --items data/game/normalized/items.json \
  --transports data/game/normalized/transports.json \
  --components data/game/normalized/logistics-components.json \
  --validator target/release/aic-witness-probe \
  --output-directory target/chain-m3-battery-verification-replay
```

Final measured artifacts are under `target/chain-m3-<name>-controls-release/<family>/run/`.
Each run records provenance hashes, component order, proposal/access/route decisions, immutable
per-validation witnesses and probe/process results, and a retained witness. Only completed runs
emit `factory-witness.json`, a complete score and `index.html`. Final independent results are
under `target/chain-m3-<name>-release-verification/`; browser checks/screenshots are under
`target/chain-m3-<name>-release-browser/`. These are local generated artifacts, not source fixtures.

| Selected witness | File SHA-256 |
| --- | --- |
| Battery/right | `663d89e05ce97b307e31a67120b8d84345466af3632e3b4cb6c740f9b727ba3e` |
| Citron/fold | `e776a09c5f41bf66355b59b20f0c37933b08e11ab64d17b1e7ef10f000b36043` |
| Iron-nine/fold | `03f3bf45d9ec8407209340ef238d47b452ff8a536baa22a7d230eac776743884` |
| Iron-eight/fold | `27ef99e43b7d9dca27971c0e869c69b52095961c784e54c249b5e503f462b372` |

Earlier hypothesis runs remain under `target/chain-m3-<name>-initial/`, `-access/`, `-aligned/`,
`-clearance/`, and `-priority-controls/`. The isolated predicate ablation is
`target/chain-m3-battery-no-clearance-controls/`. Their source hashes distinguish historical
implementations from the final rerun; they must not be presented as current default results.

## Verification and remaining risk

Fifteen factory tests cover full geometry/rate acceptance for all four graphs; induced-edge
routing before every private expansion; atomic cycle closure; preservation of prior geometry;
exact multi-lane exports; reordered arrays and renamed IDs including `__proto__`/`constructor`;
access-counterexample classification; shared budget and late-result retention; cycle/boundary
rejection; missing internal/external routes; undeclared external identities; missing/extra
inventory; changed lane rates and utilization; wrong boundary sides; inventory mismatch;
malformed/unsupported graphs; standalone verification; and immutable success/stopped CLI output.

The missing-route, changed-rate and changed-inventory negative controls deliberately pass
partial geometry and fail original-request coverage. A complete recipe-ledger pass with an
undeclared external identity also fails coverage. Thus the acceptance claim is not inferred
from a partial probe success flag. A new CLI test initially expected only a retained seed after
one proposal; the trace correctly showed a routed singleton transaction could also commit.
The assertion was corrected to require both validated commits, without changing construction.
During initial standalone CLI verification, a local variable shadowing `process` caused a
pre-run exception; it was renamed, and both success/failure CLI paths are regression-tested.

Verification commands: `node --test docs/benchmarks/constructive-*/*.test.mjs
docs/benchmarks/user-layout-reconstruction/*.test.mjs docs/benchmarks/chain-prototype/*.test.mjs`
(331 pass); `cargo check --workspace`; `cargo test --workspace --quiet` (524 pass, two existing
ignored). No Rust edit or dependency change was made. Native browser checks used the existing
`viewport-browser.mjs` on all four selected pages: 1400/736/360 widths in dark/light themes,
24 passing cases for focus, wheel zoom, drag pan, fit and overflow. All four full screenshots
were personally inspected: facility labels, external endpoints and the combined-check scope
remain readable. The long iron layouts are valid but visually poor, not claimed compact.

The slice was compared with the accepted constructive-planner design and AGENTS policy before
commit. It retains connected transactional growth, explicit pending boundaries, logical networks
by item/kind, dedicated routing, measured bounds and no exact fallback. It supports only a
single transport kind and a bounded component-tree graph class; reconvergent DAGs, larger SCCs,
multi-capacity internal edges, shared physical trunks and logistics-component synthesis remain
unsupported. Current seed orientation is deterministic, and private replay cannot undo an older
committed supplier. No claimed local proof or custom propagator was introduced.

The human references omit external routes and were not produced under these work caps. Their
smaller areas cannot establish a fair quality gap against these full factories. Nevertheless,
the generated iron/citron columns visibly fail to reproduce the human examples' balanced folding.
Next: bounded connected-chain rollback and actual routed continuation, with remaining-body fit
as a separate heuristic, a shared trial account, and a complete incumbent retained on failure.
Then compare matched inventories/boundaries under equal total budgets and isolate proposal from
evaluation effects. These steps, and the final research recommendation, remain required.
