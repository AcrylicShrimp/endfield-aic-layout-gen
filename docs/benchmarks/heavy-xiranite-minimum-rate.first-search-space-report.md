# Heavy Xiranite Minimum-Rate Search-Space Report 1

## Question

Where does the current exact joint placement-and-routing formulation first become large as the
Heavy Xiranite production graph grows, and which currently visible model families account for that
growth?

This is the first interactive research checkpoint. It does not select or apply a reduction.

## Scenario

- Workload: `heavy-xiranite-minimum-rate`
- Target: `item-xiranite-enr-powder`, quantity 1 per 10,000 ms
- Production graph: 59 facility instances, 96 logical wiring edges
- Diagnostic request ceiling: 50 by 50
- Formulation: `joint-lexicographic-layout-v4`
- Solver: Pumpkin 0.5
- Exact search budget: 5,000 ms shared by the current cumulative SCC harness

The 50 by 50 ceiling is only this report's comparison scenario. It is not part of the workload
identity, a required blueprint size, or a canonical game limit.

Artifacts:

- `heavy-xiranite-minimum-rate.static-search-space.50x50.json`
- `heavy-xiranite-minimum-rate.exact-baseline.50x50.5s.json`

## Method And Coverage

The read-only analyzer runs the existing recipe, contextual throughput, facility-instance wiring,
capacity splitting, commodity normalization, and SCC growth pipeline. It stops before constructing
a Pumpkin model.

For every cumulative SCC phase it estimates the variable families whose creation follows directly
from the current dense formulation. The estimate includes placement and endpoint literals, dense
route cells and arcs, integer flow and route-order variables, terminal presence/arm auxiliaries,
splitter/converger candidates, and bridge ownership. It excludes objective auxiliaries, exact
constraint totals, factor-graph incidence, measured placement-routing coupling, and symmetry.
Therefore every reported total is explicitly a **partial lower bound**, not the exact total model
size.

The analyzer's phase-zero estimates were cross-checked against a fresh exact run. Every family that
already has an `ExactModelMetrics` counter matched exactly.

## Exact Phase-Zero Result

The current harness did not complete phase zero, which contains one facility:

| Observation | Result |
| --- | ---: |
| Facilities | 1 |
| Commodity networks | 3 |
| Network terminals | 8 |
| Model construction | 1,897 ms |
| Search | 5,072 ms |
| First incumbent | none |
| Termination | `unknown`, unproven |
| Crossing constraints | 404,960 |

No heuristic fallback ran. This is evidence that the faithful baseline is already difficult at the
first cumulative phase under this request ceiling; it is not an infeasibility result.

### Phase-Zero Variables

| Family | Static estimate | Existing exact counter | Notes |
| --- | ---: | ---: | --- |
| Placement | 8,464 | 8,464 | Exact match |
| Facility endpoint options | 132,480 | 132,480 | Exact match |
| Route cells | 7,500 | 7,500 | 2,500 cells times 3 networks |
| Directed route arcs | 29,400 | 29,400 | Exact match |
| Integer arc flow | 29,400 | 29,400 | Exact match |
| Route order | 7,500 | 7,500 | Exact match |
| Terminal presence and directional arms | 120,000 | not counted | 16 per cell and network in current code |
| Splitter/converger candidates | 60,000 | 60,000 | Exact match |
| Bridge selection | 5,000 | 5,000 | Belt and pipe layers |
| Bridge rotations | 20,000 | 20,000 | Exact match |
| Crossing owners | 15,000 | 15,000 | Two axes per network and cell |
| Covered lower bound | 434,744 | n/a | Excludes objective auxiliaries |
| Objective auxiliaries | excluded | 117,633 | Requires authoritative recorder integration |

The existing metric families sum to 432,377 variables when objective auxiliaries are included but
terminal presence/arm auxiliaries are omitted. Combining the disjoint known groups identifies at
least 552,377 phase-zero variables. The exact total remains unknown until every variable creation is
recorded authoritatively.

## Full-Graph Static Lower Bound

The production graph has 41 SCCs under the current one-ready-SCC scheduling policy. Eight SCCs are
cyclic, the largest cyclic or acyclic SCC contains five facilities, condensation depth is eight,
and the logical facility graph is one weakly connected component.

At the final 59-facility phase, the covered variable lower bound is **9,162,080**. Objective
auxiliaries and any currently uncounted variable families would increase this total.

| Covered family | Variables | Share of covered lower bound |
| --- | ---: | ---: |
| Facility endpoint options | 7,188,336 | 78.46% |
| Terminal presence and directional arms | 640,000 | 6.99% |
| Placement | 515,144 | 5.62% |
| Splitter/converger candidates | 320,000 | 3.49% |
| Directed route arcs | 156,800 | 1.71% |
| Integer arc flow | 156,800 | 1.71% |
| Crossing owners | 80,000 | 0.87% |
| Route cells | 40,000 | 0.44% |
| Route order | 40,000 | 0.44% |
| Bridge rotations | 20,000 | 0.22% |
| Bridge selection | 5,000 | 0.05% |

The 197 facility-side endpoint groups have 8,280 to 49,680 legal placement-port options each; the
median is 41,400. This is why endpoint literals dominate the final static estimate. External
terminal options reuse their counterpart facility options and do not create a second literal set.

All non-placement and non-endpoint covered families together contain 1,458,600 variables. This is
still substantial, but much smaller than the endpoint encoding in the full graph.

## Growth Hotspots

The largest covered-variable increases occur when a cyclic SCC introduces several facilities or a
new commodity network:

| Phase | Added facilities | Added networks | Cumulative facilities | Cumulative networks | Covered increase | Covered total |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | 1 | 3 | 1 | 3 | 434,744 | 434,744 |
| 1 | 1 | 3 | 2 | 6 | 343,504 | 778,248 |
| 2 | 1 | 2 | 3 | 8 | 320,144 | 1,098,392 |
| 16 | 1 | 2 | 17 | 12 | 386,356 | 2,637,580 |
| 30 | 3 | 1 | 33 | 15 | 528,992 | 5,213,296 |
| 34 | 5 | 1 | 47 | 16 | 959,920 | 7,491,392 |
| 40 | 3 | 0 | 59 | 16 | 439,392 | 9,162,080 |

Introducing a new network adds dense grid-wide route, flow, order, arm, component, and crossing
families even when only one facility was added. Introducing facilities adds thousands of placement
literals and, more importantly in later phases, many placement-port endpoint literals for every
incident capacity-split requirement.

## Supported Conclusions

1. Incremental SCC growth does not make the first exact model small under the 50 by 50 scenario.
   Phase zero already contains at least 434,744 covered variables and 404,960 recorded crossing
   constraints.
2. The first-phase difficulty is not caused primarily by placing the one facility. Placement is
   only 8,464 variables. Dense per-network grid auxiliaries, endpoint options, component candidates,
   and crossing rules are much larger.
3. In the full graph, the current placement-port endpoint encoding is the dominant covered variable
   family at 78.46%.
4. New commodity networks create a fixed grid-wide cost. Cyclic SCCs containing three to five
   facilities create the largest later jumps.
5. A decomposition boundary cannot be judged only by facility count. Network introduction,
   facility-side endpoint groups, and frontier coupling must be included.

## Not Yet Supported

- The report does not prove that the largest variable family consumes the most search time.
- It does not yet measure exact constraint arity, factor-graph degree, separators, symmetry, or
  placement-routing cross incidences.
- It does not show whether a different exact endpoint encoding improves propagation.
- It does not establish an exact or heuristic decomposition boundary.
- It does not show solution-quality loss for any reduction because no reduction was applied.
- It does not establish that 50 by 50 is the best or necessary request ceiling.

## Candidate Next Research Actions

### A. Authoritative model recorder

Record every variable domain, posted constraint family, arity, coefficient, and family incidence in
the actual builder. This would determine whether phase-zero search is dominated by crossing
constraints, endpoint coupling, weak big-M propagation, or another currently invisible structure.
It is the strongest causal follow-up but requires touching every formulation construction site.

### B. Bound-sensitivity experiment

Run the unchanged exact phase-zero model under several explicit request ceilings and fixed time
budgets. This would measure how dense-grid growth affects construction and first-feasible behavior,
but smaller ceilings may also remove feasible layouts and therefore cannot be interpreted as a safe
reduction.

### C. Exact endpoint-encoding study

Design a semantics-preserving coordinate/rotation/port formulation that avoids one Boolean for every
placement-port combination, then prove small-case objective equivalence before implementation. The
static data gives this high potential for full-graph size, but current evidence does not establish
its effect on phase-zero search.

No candidate has been selected by this report. The next research action requires user review and
explicit approval.
