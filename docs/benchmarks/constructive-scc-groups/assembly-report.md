# Variable-membership assembly: implemented, not promoted

Status: assembly and atomic dependent-SCC growth are verified research capabilities.
**No new complete factory in 32 closure attempts.** The independently revalidated complete
incumbent stays **3,224 area / 775 transport cells / 119 turns**, all 59 facilities.
This result does not establish satisfactory compactness or finish the broader goal.

## Implemented contract

- A bank contains a mandatory source SCC and optional input-closed extensions. Alternatives
  may cover different facility sets; their union of possible owners is disjoint from every
  other bank. No supplier is cloned, omitted or silently converted to an external source.
- Pack exact facility/belt/pipe occupied rows, allowing independent transport layers.
  Pending outputs may face any side. Demand uses exact remaining commodity margins and
  catalog line capacities, not one port for every fragmented logical edge.
- A coverage-stratified beam retains smaller stop states and larger extensions. Comparing
  their raw partial areas alone would discard the latter unfairly. Envelope diversity is
  retained inside coverage strata. Both are bounded heuristics, not dominance proofs.
- Before ordinary frontier closure, introduce a ready dependent SCC through a checked
  superset variant of its source bank. Reseat only that bank while preserving other bank
  geometry, apart from a common normalization translation. Add exactly the required atom
  and validate the resulting region. This phase precedes any inter-bank routed connection;
  it must not be reused to detach a bank after outside routes have attached.
- The complete incumbent is independently checked at the start, and only a complete,
  independently validated improvement may replace it. `newly_complete` is separate from
  `factory_validated`: the latter can describe the retained incumbent, not new construction.

This removes the previous fixed-membership/east-output/two-large-cluster requirement from
this experiment. It does not replace the production Rust constructor or claim a general
atomic-cycle geometry solver: dependent growth needs an available source-bank template.
The actual F33/F46 dependency is now introduced together when a smaller F32/F45 seed was
selected. All 24 such atomic expansions in the final matrix validate.

## Visual defect caught during packing

The first four-candidate run admitted physically nonoverlapping 32/33-facility packings
that stranded F41, F42 and F43 outputs inside closed pockets. Their connection cells were
free, but the spaces contained only 22, 27 and 13 cells, respectively, and could not fit
the required 6x4 or 4x6 downstream footprint. The actual 33-facility render was inspected.

Closed-pocket checks now run while selecting each packing beam, not only after final
selection. Rejected candidates are replaced from a bounded alternate shortlist (16 times
the target beam/candidate count). The current 32 initial packed states all pass physical,
recipe-margin and pending-interface checks. This preserves necessary access; it does not
guarantee that later routing leaves other demands accessible or that all consumers can
be placed simultaneously.

One unit regression constructs a nearly closed 3x3 ring with a pending output inside it.
The geometrically snug final cell would close that output's only exit. The packer rejects
the nine-area placement and retains a larger accessible alternative. This is not an
instruction to enlarge the factory by arbitrary fixed spacing.

## Controlled 2x2 matrix

All cases use the same aligned graph, 69 independently rechecked source variants, an
eight-state packing beam, eight final candidates, two closure workers, 64 ordinary
placement trials per frontier, and the same two-sided placement family. Only source-bank
order and corridor family vary. Initial and post-atomic witness hashes match exactly
between corridor controls for each order.

| Bank order | Corridor family | New completions | Maximum covered | Packing time | Full matrix wall time |
| --- | --- | --- | --- | --- | --- |
| Largest first | Simple | 0/8 | 43/59 | 3.507s | 49.305s |
| Largest first | Perimeter | 0/8 | 43/59 | 3.405s | 78.986s |
| Smallest first | Simple | 0/8 | 40/59 | 2.210s | 41.696s |
| Smallest first | Perimeter | 0/8 | 40/59 | 2.262s | 75.999s |

These are observed whole-portfolio times under concurrent matrix execution, not single
factory-generation times or a controlled CPU microbenchmark. Generation and closure
costs remain separate in the JSON reports. Source hashes are captured at process start.

Largest-first retains initial membership counts 16,31,17,30,18,29,19,28; smallest-first
retains 16,33,17,32,18,31,19,30. Stop states are not silently excluded because they need a
dependent SCC. All initial states and all required atomic extensions pass validation;
failure occurs later in the ordinary frontier completion stage.

The furthest candidate has 43 facilities in **144x25 / 3,600 area**. Its actual render
shows a long horizontal row, long lower/upper commodity corridors and the inherited
large reciprocal return loops. It is visibly poor and already larger than the complete
3,224-area incumbent. Do not promote it, celebrate packing fill as a whole-factory gain,
or present the 43-facility result as a complete graph.

## A* counterfactual: two different failure modes

The new `route-audit.mjs` holds one recorded failed placement and every existing body/wire
fixed, then compares all unused source/target port pairs. It only accepts exact one-input,
one-capacity-bounded-lane replays; F23's two-input case is explicitly rejected as outside
this replay contract.

### F56 to F50

For the first failed F50 placement in the 31-facility largest-first candidate:

- Simple and perimeter corridor templates find zero paths across 20 port pairs.
- Ordinary A* finds paths for all 20 pairs in the measured extent plus four cells.
- All 20 constructed witnesses pass independent geometry and covered recipe margins.
- All 20 fail the pending-interface check. The same placement before adding the path
  passes that check, isolating the newly installed route as the cause of this rejection.
- The paths have 40..47 steps. The displayed candidate is a 32-facility **rejected**
  transaction, not a new accepted prefix or complete factory.

The native image highlights the actual new F56-to-F50 route in red. It travels through
the central passage and around the bottom/right of the packed banks. That wire closes
other pending outputs into narrow 54-cell and 17-cell components. For the displayed
candidate these affect F43->F29, F42->F28/F29, F22->F16, F27->F19 and F25->F18. None of
those components can accommodate its required next consumer footprint. This was checked
numerically and inspected in the rendered geometry.

Thus simple templates are incomplete for this fixed placement, but replacing them with
A* alone does not make this transaction acceptable. This does not prove all alternative
paths or placements fail; rip-up, relocation, another port/route order and planned
structural crossings remain outside the replay.

### F38 to F26

A second one-input replay tests 15 pairs. Every A* pair has a blocked endpoint. Its
proposed new facility already obstructs pending output access before a route is added.
This is a placement/access defect, not evidence of A* search exhaustion. The two cases
must not be conflated as simply "the router cannot find a path."

## Rendering and verification

- Corrected the native viewer's `used bounds` label: an unused origin margin is not
  factory area. The score was already origin-independent; the old display used maximum
  coordinates rather than width/height of the occupied bounding box. Include component
  positions and both layers. A unit test covers translated geometry and component bounds.
- The viewer now distinguishes independent geometry PASS from planner REJECTED for the
  rejected route replay. Physical validity alone is not future-interface acceptance.
- 126 related Node tests pass, including six packing/atomic/ownership/interface tests
  and the bounds-rendering regression. Tests include input immutability, optional-owner
  overlap, missing mandatory members, open-input variants, translated concave shapes,
  north-facing outputs, coverage diversity and missing/bounded atomic templates.
- `cargo check --workspace` passes. `cargo test --workspace`: 524 pass, no failures,
  two existing ignores. No Rust changes.
- The retained complete factory again passes all eight independent audit controls
  against the original graph. Each matrix also rechecks it against the aligned graph.
- Actual rejected packing, 33-facility accessible assembly, 37/43-facility stopped
  prefixes and the highlighted rejected A* witness were personally viewed. Browser
  checks cover six theme/viewport combinations, plus zoom/pan/Fit for closure views.
- Compared with `AGENTS.md` and the accepted constructive-planner design: this is
  isolated research over already routed valid regions, with immediate validation,
  exact pending obligations, no production cutover, no global exact fallback, no
  factory W/H constraint and unchanged complete-factory area/tile/turn priority.

## Reproduction and artifacts

Use the SCC preparation command in `report.md` first. With that inventory and the
existing release validator, run each order/family combination in a fresh directory:

```sh
node docs/benchmarks/constructive-scc-groups/assemble.mjs \
  --graph target/scc-stop-grow-final/graph.json \
  --regions target/scc-stop-grow-final/regions.json \
  --incumbent artifacts/benchmarks/constructive-shape-packing/full-matrix.json \
  --recipes data/game/normalized/recipes.json \
  --facilities data/game/normalized/facilities.json \
  --items data/game/normalized/items.json \
  --transports data/game/normalized/transports.json \
  --components data/game/normalized/logistics-components.json \
  --validator target/release/aic-witness-probe \
  --beam-width 8 --candidate-limit 8 \
  --order largest-first --route-family simple \
  --output-directory target/scc-assembly-replay
```

Final matrices: `assembly-largest-simple.json`, `assembly-largest-perimeter.json`,
`assembly-smallest-simple.json`, `assembly-smallest-perimeter.json`. Audit evidence:
`route-audit-f50.json`, `route-audit-f26.json`, `assembly-incumbent-audit.json`.
Detailed witnesses, interfaces, validators, closure traces and process logs remain in
their recorded output directories. The F50 audit reuses an earlier identical 31-facility
packing; final largest-first controls reproduce its packed witness hash.

Existing Tailscale pages:

- `bound-interconnect-scc-assembled.html`: valid 33-facility partial assembly.
- `bound-interconnect-closure-scc-largest-partial.html`: furthest 43-facility prefix.
- `bound-interconnect-closure-scc-route-audit.html`: rejected A* transaction; red overlay
  in the chat screenshot identifies its newly added path.

## Next decision

Do not infer that the user's SCC/density idea is disproven. This implementation still
packs source banks before assigning their shared downstream interconnect and then uses
the old singleton, fixed-orientation closure. That interface between stages is inadequate
in the measured examples. A free port and an individually open pocket do not reserve
capacity for multiple later paths or establish compatible merge/crossing order.

Next, plan the common merge/crossing interfaces and downstream consumer slots while
choosing module positions/rotations, preserving those connection corridors through
growth. Test alternative local placements/routes where A* exposes a template limitation,
but do not accept a path that strands other obligations. Retain cut/shape alternatives
and the complete incumbent. The inherited larger source-loop geometry also still needs
reconstruction; the current projection is not a minimum geometry solver.

This is actionable unfinished implementation, not an external blocker. Satisfactory
compactness, additional complex recipes, code cleanup and the web workflow remain open.
