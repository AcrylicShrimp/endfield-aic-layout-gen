# Heavy Xiranite Exact Dimension Partition Report

## Outcome

Fixing the actual used width and height removes the current first-witness cliff for the smallest
failing two-network model.

The unfixed `pair-0-1` feasibility baseline found no incumbent in 5,001 ms. The exact `5x7`
partition found and validated a complete witness in 26 ms. Both models contain the same 10,970
variables. The fixed model differs only by two equality constraints and two terms:

```text
used_width = 5
used_height = 7
```

This confirms that making the solver choose used dimensions while those dimensions also select
external boundary-terminal positions and depend on routed geometry is the immediate search cliff.
It does not yet distinguish weak propagation from an unfavorable default branch order inside that
coupled disjunction.

## Exact Candidate Domain

The 12 by 12 request dimensions remain hard ceilings, not target blueprint dimensions. Candidate
enumeration starts from proven game-rule lower bounds rather than from `1x1`:

| Proven necessary condition | Value |
| --- | ---: |
| Minimum width containing every facility | 5 |
| Minimum height containing every facility | 5 |
| Total facility area | 25 |
| Minimum non-facility transport cells | 1 |
| Minimum used area | 26 |

The transport-cell bound follows from two existing rules: the selected problem contains transport
networks, and transport cannot occupy a facility cell. It does not estimate packing quality.

After applying only these proven bounds, 63 exact `(width,height)` candidates remain. They are
ordered by area, maximum side, width, and height. Ordering changes execution order only; the
machine-readable case artifacts retain the complete candidate list.

## Controlled Difference

Every case uses:

- Heavy Xiranite cumulative SCC phase zero;
- network indices `0,1`;
- the factored shared-boundary-terminal formulation;
- the same 12 by 12 request ceilings;
- feasibility-only Pumpkin search;
- the same five-second search budget;
- unrestricted facility placement and rotation;
- unrestricted compatible port selection;
- unrestricted external-terminal side and boundary cell;
- unrestricted shared belt routing, flow, topology, capacity, and collision decisions.

Only the actual used width and height are fixed. The fixed model has no extra variables and retains
all original objective-definition state.

## Results

| Actual used size | Area | Build | Search | First incumbent | Result | Proof | Peak RSS |
| --- | ---: | ---: | ---: | ---: | --- | --- | ---: |
| unfixed, ceiling 12x12 | unknown | 67 ms | 5,001 ms | none | unknown | unproven | 85.14 MiB |
| 5x6 | 30 | 83 ms | 47 ms | none | infeasible | proven | 81.00 MiB |
| 6x5 | 30 | 72 ms | 25 ms | none | infeasible | proven | 79.55 MiB |
| 5x7 | 35 | 76 ms | 26 ms | 26 ms | feasible | witness validated | 84.50 MiB |

All fixed cases contain 10,970 variables, 33,258 constraints, and 109,135 terms. The unfixed model
contains 10,970 variables, 33,256 constraints, and 109,133 terms. The complete difference is the
two research equalities.

The successful witness contains:

- one 5 by 5 Xiranite Oven at `(0,1)`;
- used bounds exactly 5 by 7;
- four physical belt tiles;
- zero route turns;
- zero logistics components;
- one direct enriched-Xiranite-powder output boundary tile;
- three Xiranite-powder input tiles, including one short one-segment connection and direct
  terminal-to-port cells.

No routing cell overlaps the facility, and the normal independent witness validator passed.

## Lower And Upper Bounds

Before search, game rules prove only an area lower bound of 26. The first two executable candidates
are `5x6` and `6x5`, both area 30, because width and height must each be at least five. Pumpkin proved
both partitions infeasible.

The validated `5x7` witness proves an area upper bound of 35. There is no other legal integer
dimension area between 30 and 35 under the proven width and height bounds. Therefore the primary
used-area optimum for this exact `pair-0-1` problem is **proven to be 35**.

This does not prove the full lexicographic optimum. The unexecuted `7x5` area-35 partition could
produce a different transport-tile or turn result, and feasibility-only search does not optimize
those secondary values.

## Root Cause

The evidence now forms a narrow causal chain:

1. every single network previously solved;
2. every two-network subset previously missed its first incumbent in five seconds;
3. disabling optimization did not change that behavior;
4. fixing only the two used-dimension variables restores a complete two-network witness in 26 ms;
5. the model variable set is unchanged and the witness passes the same validation.

The immediate cliff is therefore the solver-facing disjunction over actual used dimensions and its
coupling to boundary terminals and route geometry. It is not model construction, memory pressure,
secondary objective improvement, or a missing feasible layout.

The experiment does not prove which implementation mechanism is responsible inside that boundary:

- Pumpkin may branch on low-level route and flow state before resolving width and height;
- the bounding-box maximum constraints may propagate too weakly into boundary-terminal keys;
- boundary-terminal choices may propagate too weakly back into route geometry;
- several of these effects may combine.

Exact dimension partitioning removes the whole circular choice from each solver instance, so this
experiment intentionally does not separate those sub-causes.

## Improvements Present In This Baseline

The measured result includes the exact formulation improvements made before this diagnosis:

- one shared physical belt layer and one shared physical pipe layer instead of one dense grid per
  logical line;
- independent placement choice and port choice variables instead of flattened placement-port
  candidates;
- canonical facility occupancy shared with both transport layers for exact collision propagation;
- solver-selected same-item commodity networks with shared trunks, splitters, and convergers;
- external inputs and outputs represented as terminals of those same networks rather than
  hand-shaped straight connectors;
- external terminals constrained to the actual used bounding-box boundary;
- circulation permitted, with compact physical transport tiles remaining an objective concern;
- complete independent witness validation and self-contained HTML failure/success artifacts;
- separate feasibility-only diagnosis proving that the previous cliff occurs before optimization.

The current fixed-dimension path is research-only and does not alter production solving.

## Recommended Next Decision

The next architecture candidate is an **exact dimension portfolio** around the unchanged joint
placement-routing model:

1. calculate proven lower bounds from game rules or a sound relaxed model;
2. run independent exact `(width,height)` partitions, naturally parallelizable;
3. turn every validated witness into an area upper bound;
4. stop launching cases above that upper bound for the primary objective;
5. prove every smaller-area case infeasible before claiming area optimality;
6. compare all same-area candidates needed for secondary objectives.

This is semantics-preserving when the portfolio retains every candidate not excluded by proof. It
does not pre-place a facility, choose a port, restrict a corridor, or replace routing with a
heuristic.

Before making it production orchestration, the next interactive experiment should apply this
portfolio to the next cumulative graph growth point. That will show whether dimension partitioning
merely solves the two-network micro-case or moves the cliff far enough to expose the next coupling
blocker. A stronger lower-bound pre-pass can then be evaluated separately by comparing its cost and
the number of exact dimension cases it safely removes.

## Verification

- experiment contract committed before implementation;
- `cargo fmt --all`;
- `cargo check --workspace`;
- `cargo test --workspace`: 16 CLI tests and 166 data tests passed;
- `cargo build --release --workspace`;
- three isolated optimized release processes with equal 5,000 ms budgets;
- complete candidate list serialized in every fixed case;
- fixed/unfixed variable counts compared;
- exactly two added `research-fixation` equalities verified by unit test and model metrics;
- successful fixed witness independently validated;
- no production solver, brancher, objective, placement, port, or routing behavior changed.

## Artifacts

- Contract: `docs/designs/exact-used-dimension-partition-diagnosis.md`
- Machine-readable comparison:
  `docs/benchmarks/heavy-xiranite-exact-dimensions/comparison.json`
- Proven-infeasible `5x6` JSON and HTML:
  `docs/benchmarks/heavy-xiranite-exact-dimensions/5x6.json` and `5x6.html`
- Proven-infeasible `6x5` JSON and HTML:
  `docs/benchmarks/heavy-xiranite-exact-dimensions/6x5.json` and `6x5.html`
- Validated `5x7` JSON and HTML:
  `docs/benchmarks/heavy-xiranite-exact-dimensions/5x7.json` and `5x7.html`

## Decision Boundary

The used-dimension partition diagnosis is complete. Do not implement production portfolio
orchestration or start the next cumulative growth experiment until the user reviews this result.
