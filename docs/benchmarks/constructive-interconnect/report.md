# Catalog limits and planned lane permutations

## Catalog census

Read every runtime catalog: 305 recipes, 28 facility definitions, 538 items, two transports and six
logistics components. `result.json` includes every recipe's exact rates, per-item lane lower bounds,
port budgets, maxima and all witnesses, plus every facility's port counts and component arms.

| Quantity per recipe at full utilization | Input maximum | Output maximum |
| --- | ---: | ---: |
| Distinct items, all transports | 2 | 2 |
| Distinct belt items | 2 | 1 |
| Minimum belt lanes, summing per-item ceilings | 5 | 3 |
| Distinct pipe items | 2 | 2 |
| Minimum pipe lanes | 2 | 2 |

The belt input maximum occurs at `tools-proc-battery-2-1`, `tools-proc-battery-3-1` and
`tools-proc-battery-5-1`. Across facility definitions, belt input/output port maxima are six/six and
pipe maxima three/two. These are separate maxima, not simultaneous requirements for every facility.
No recipe has a necessary port-count deficit in this snapshot. Counts alone do not certify geometry.

The user is correct that **one consumer recipe's distinct ingredient count** bounds how many
commodity types its intake block must handle. It does not bound physical lane count, replicated
group size or the number of order inversions at fixed group boundaries. Even two item types can
require arbitrarily many lanes when consumer instances are replicated. There is no catalog-only
finite global crossing-count bound. Allowed component directions are not a count of independent
channels: the existing validator permits two perpendicular straight commodity channels at a bridge.

## Structural decomposition and scope

An N:M interconnect can contain allocation, fanout/fanin and a lane-order permutation stage. Those
are not semantically interchangeable: only compatible same-item flows may merge; independent
crossing channels retain identity. This milestone implements the **permutation stage only**.
Recipe-derived examples expose already-expanded boundary lanes. They do not place facilities,
bind actual ports or claim full N:M synthesis.

The compiler deterministically moves each requested target lane into position by adjacent swaps.
Each swap occupies a pre-reserved 6 x 6 rectangle; incoming rows are separated by three cells.
One lane descends, the other takes a two-cell lower detour and ascends through a preplanned catalog
bridge. Other lanes pass straight outside the rectangle. Stages are disjoint in x. There is no A*,
CP-SAT, opportunistic bridge insertion or global grid search.

Each adjacent exchange reduces one inversion, so a finite permutation can be decomposed this way.
For K independent distinct-item lanes there are at most K(K-1)/2 swaps; stable two-item orders need
at most nA*nB. Same-item lane inversions are rejected in this slice: resolve compatible allocation
upstream rather than introduce a self-crossing the production validator does not represent.
Diagnostic limits of 10,000 swaps and a conservative 200,000 path cells are resource guards, not
game rules or global infeasibility proofs. No finite catalog-specific maximum is hardcoded.

The cell checker is independent of the swap sequence: it checks unit steps, loops, boundary order
and direction, item/transport identity, per-lane capacity, reserved envelope occupancy and every
shared cell. A shared cell must have a preplanned matching bridge with exactly two different-item,
perpendicular straight channels. Rates fit both the line and bridge capacity. This checker is
**not independent production-validator acceptance**, which remains an explicit output gate.

## Measured examples

| Actual recipe-derived boundary | Consumers | Lanes | Bridges | Reserved envelope |
| --- | ---: | ---: | ---: | --- |
| `thickener-plant-moss-enr-powder-2-1`, utilization 1/4 | 2 | 4 | 1 | 6 x 12 |
| `tools-proc-battery-2-1`, utilization 1 | 2 | 10 | 6 | 36 x 30 |

All rates derive from runtime catalogs and the external fixture spec. The source boundary is
grouped by item, and the target boundary by consumer. The small case uses different actual powder
items, so same-item flow reassignment cannot substitute one for the other. The large case has two
iron-component lanes and three originium-powder lanes per consumer. No actual supply facilities or
fanout trees are implied by the boundary ordering. These are not minimum-area certificates or proof
that a freely placed factory requires exactly these bridges.

Frozen single sequential run: census 16.57 ms, both examples plus geometry checking 6.80 ms. Node
process startup, catalog IO, report serialization, rendering, builds and tests are excluded. This
is a small diagnostic measurement, not a production-generation benchmark.

Actual rendered four-lane, ten-lane and mobile images were inspected. The construction is visibly
spacious, as the user noted: serial swap stages repeat straight segments and the three-cell lane
pitch does not match adjacent facility input ports. Independent swap stages can later be packed
together, but compaction is not needed to establish the first complete interconnect contract.

## Next acceptance gates

1. Construct real supply/consumer facilities and capacity-valid splitter/converger trees; reserve
   their occupied cells and distinguish item counts from facility counts and lane counts.
2. Adapt the wide permutation boundary to concrete catalog ports without blocking other commodities.
3. Run the existing independent production network validator on the complete covered fixture, with
   negative controls removing a bridge, splitter or required capacity. External recipe obligations
   remain explicit; do not call an internally connected fixture a complete factory.
4. After correctness, compact disjoint swaps into shared stages and shrink boundary spacing while
   preserving the validated contract. Then integrate the module with the real block graph.

## Verification and reproduction

Seven Node tests include all 873 distinct-item permutations through six lanes, all stable binary
item words through six lanes, belt/pipe cases, capacity limits, malformed geometry, catalog census
and both actual recipe-derived examples. Seven previous zero-crossing tests also pass. Workspace
check and all 519 Rust tests pass with two existing ignores; no Rust source changed. Browser checks
pass at 1400/736/360 px in light/dark themes, and both case controls render the expected lane count.
Reviewed against AGENTS and the constructive design: this research CLI does not change connected
production construction, catalog data or the exact solver.

```sh
node --test docs/benchmarks/constructive-interconnect/model.test.mjs
node docs/benchmarks/constructive-interconnect/run.mjs \
  --recipes data/game/normalized/recipes.json --items data/game/normalized/items.json \
  --facilities data/game/normalized/facilities.json --transports data/game/normalized/transports.json \
  --components data/game/normalized/logistics-components.json \
  --fixture docs/benchmarks/constructive-interconnect/fixture.json \
  --report-output NEW_JSON --visualization-output NEW_HTML
```

New distinct output paths are required. Detailed data is file-only JSON; stdout is a completion
summary. The tailnet page is `http://100.114.24.86:8765/structural-interconnect.html`.
