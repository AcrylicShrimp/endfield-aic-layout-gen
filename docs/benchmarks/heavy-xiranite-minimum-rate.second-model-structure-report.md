# Heavy Xiranite Minimum-Rate Model-Structure Report 2

## Question

What model does the current exact joint placement-and-routing formulation actually post for the
first cumulative SCC phase, and where are its largest structural costs and placement-routing
couplings?

This is the second interactive research checkpoint. It measures the accepted formulation without
selecting or applying a search-space reduction.

## Scenario

- Workload: `heavy-xiranite-minimum-rate`
- Target: `item-xiranite-enr-powder`, quantity 1 per 10,000 ms
- Cumulative phase: 0, containing one facility, three commodity networks, and eight terminals
- Diagnostic request ceiling: 50 by 50
- Formulation: `joint-lexicographic-layout-v4`
- Solver: Pumpkin 0.5, release build
- Exact search budget: 5,000 ms

The 50 by 50 ceiling is only this report's comparison scenario. It is not a workload invariant, a
required blueprint size, or a canonical game limit.

Artifacts:

- `heavy-xiranite-minimum-rate.static-search-space.50x50.json`
- `heavy-xiranite-minimum-rate.model-recorded.50x50.5s.json`
- `heavy-xiranite-minimum-rate.model-recorded.50x50.5s.html`

## Recorder Coverage

Every bounded integer created by the exact formulation now passes through one recorder. Every
base-model equality, inequality, maximum, multiplication, and table constraint is posted through
the same recorder with a stable variable family and constraint family. The recorder observes the
model before search starts and does not change a variable domain, constraint expression, creation
order, search brancher, warm-start hint, or objective order.

The integrated-layout report schema is now version 16 and adds `exact.model_complexity`. The static
search-space report schema is now version 2 and reserves factor-graph family-incidence output.
Consumers of older report schemas must reject or explicitly migrate them rather than silently
assuming the new fields exist.

The counts in this report are formulation-level counts. Pumpkin can compile one posted constraint
into multiple internal propagators or SAT clauses; those solver-internal objects are not counted.
No lexicographic optimum-fixing constraint is included because the snapshot is taken before search.
Symmetry-group inference, articulation-point analysis, and retained full factor-graph export remain
unmeasured.

## Exact Phase-Zero Result

The solver again returned `unknown` without an incumbent. This remains a resource-limited result,
not an infeasibility result, and no heuristic fallback ran.

| Observation | Previous counter run | Recorded-model run |
| --- | ---: | ---: |
| Model construction | 1,897 ms | 4,106 ms |
| Search | 5,072 ms | 5,069 ms |
| First incumbent | none | none |
| Formulation variables | unknown | 552,377 |
| Formulation constraints | unknown | 2,257,028 |
| Constraint terms | unknown | 8,693,175 |
| Distinct factor-graph incidences | unknown | 8,428,215 |
| Peak resident memory | not captured | 1,785,446,400 bytes |

The recorder approximately doubled construction time in this run. Search still received the same
mathematical model and the same five-second budget, but recorder overhead must be separated from
future solver-performance comparisons. Peak memory includes the CLI, exact model, solver, and
recorder rather than the recorder alone.

## Static Estimate Accuracy

The first report's phase-zero covered lower bound was 434,744 variables. The actual total is
552,377, a difference of 117,633 variables, or 27.06% over the covered lower bound. The difference
is exactly the previously excluded objective-variable family. Every previously covered family
matches the recorder:

| Family | Static estimate | Actual |
| --- | ---: | ---: |
| Placement | 8,464 | 8,464 |
| Facility endpoint options | 132,480 | 132,480 |
| Route cells | 7,500 | 7,500 |
| Directed route arcs | 29,400 | 29,400 |
| Arc flow | 29,400 | 29,400 |
| Route order | 7,500 | 7,500 |
| Terminal presence | included in combined estimate | 60,000 |
| Directional route arms | included in combined estimate | 60,000 |
| Splitter/converger candidates | 60,000 | 60,000 |
| Bridge selection | 5,000 | 5,000 |
| Bridge rotations | 20,000 | 20,000 |
| Crossing owners | 15,000 | 15,000 |
| Objective auxiliaries | excluded | 117,633 |

The model has 510,270 two-value domains and 42,107 wider integer domains. Its summed log-domain
volume is approximately 677,525 bits. Endpoint variables are the largest single family at 23.98%,
but objective auxiliaries are close behind at 21.30%. At phase zero, endpoint variables do not
dominate the whole model as strongly as they do in the final static full-graph estimate.

## Constraint Structure

The formulation posts 2,257,028 user-level constraints. The median is small: the 95th percentile
arity is nine and the mean distinct arity is 3.73. A few global constraints are very wide: the
maximum arity is 65,001, the four endpoint-choice equalities reach 41,400 terms each, and the one
placement-choice equality has 8,464 terms.

| Constraint family | Constraints | Share | Terms |
| --- | ---: | ---: | ---: |
| Bridge crossing | 394,960 | 17.50% | 940,720 |
| Branch topology | 352,500 | 15.62% | 2,255,040 |
| Route-cell activation | 331,260 | 14.68% | 978,780 |
| Terminal presence | 324,960 | 14.40% | 854,880 |
| Turn definition | 312,996 | 13.87% | 792,540 |
| Used geometry | 234,202 | 10.38% | 687,602 |
| Route arm | 60,000 | 2.66% | 178,800 |
| Line capacity | 59,380 | 2.63% | 323,760 |
| Arc activation | 58,800 | 2.61% | 117,600 |
| Other families combined | 127,970 | 5.67% | 1,563,453 |

Two different forms of size are therefore visible:

1. A very large count of mostly local routing, component, and objective-definition constraints.
2. A small number of very high-arity placement, endpoint-choice, bounding-box, and objective-sum
   constraints.

This report does not claim which form dominates Pumpkin runtime. It only establishes that variable
count alone is not a sufficient description of this model.

## Placement-Routing Coupling

The formulation factor graph is one connected component even though phase zero has only one
facility and three commodity networks. It contains 552,377 variable vertices, 2,257,028 constraint
vertices, and 8,428,215 distinct incidences. Variable degree has mean 15.26, 95th percentile 22,
and maximum 141.

Of all posted constraints:

- 2,061,137, or 91.32%, touch more than one variable family.
- 981,040, or 43.47%, directly couple a placement-side variable (placement or endpoint choice) to a
  routing-side variable.
- Those direct placement-routing constraints contain 4,622,236 distinct incidences.
- 399,960 constraints belong to bridge-crossing or transport-collision families.
- Objective-related families contribute 547,207 constraints and 1,562,655 incidences.

The largest family-to-family incidence counts include:

| Variable family | Constraint family | Incidences |
| --- | --- | ---: |
| Endpoint | Branch topology | 1,059,840 |
| Branch component | Branch topology | 540,000 |
| Endpoint | Terminal presence | 529,920 |
| Objective | Turn definition | 503,244 |
| Placement | Used geometry | 423,200 |
| Placement | Transport collision | 423,200 |
| Route arm | Branch topology | 420,000 |
| Endpoint | Route-cell activation | 397,440 |
| Route cell | Route-cell activation | 331,260 |
| Route arc | Turn definition | 289,296 |

This directly confirms that the authoritative model is not a placement candidate selector followed
by an independent router. Placement/port choices, route occupancy, component topology, collision,
and objective geometry are connected inside one constraint model. It also explains why logical SCC
growth alone does not make the first physical model separable: dense shared-grid collision and
objective constraints reconnect the network-specific structures.

## Supported Conclusions

The evidence now supports these conclusions:

1. The first phase contains exactly 552,377 formulation variables; the first static lower bound was
   accurate for every covered family.
2. The actual phase-zero model is dominated structurally by millions of local routing/component
   constraints plus a few very wide global choices and sums.
3. Objective encoding is already material before optimization begins: it contributes 117,633
   variables and roughly one quarter of posted constraints.
4. The factor graph is a single connected component with extensive direct placement-routing
   coupling. A decomposition based only on ordinary connected components would find nothing to
   split in this phase.
5. The exact baseline still produces no incumbent in five seconds. The recorder does not establish
   whether endpoint encoding, local routing constraints, global high-arity constraints, objective
   auxiliaries, or their interaction is the runtime bottleneck.

The evidence does **not** yet justify restricting placement candidates, cropping routing space,
preselecting ports, weakening routing, or changing solver architecture. Those would remove or alter
the approved research subject without a controlled experiment.

## Next Decision Gate

The next experiment should be selected interactively. The measured structure makes several
different questions defensible:

- **Bound sensitivity:** repeat the same phase over smaller and larger caller-supplied ceilings to
  measure which variable and constraint families scale with grid area, perimeter, or placement
  origins.
- **Endpoint formulation:** isolate the four very high-arity endpoint choices and their 132,480
  variables, then compare exact, semantics-preserving encodings.
- **Objective formulation:** isolate the 117,633 objective auxiliaries and 547,207
  objective-related constraints, then test exact equivalent encodings or staged objective model
  construction.
- **Routing-local formulation:** analyze the largest local constraint families and test stronger or
  smaller exact encodings without restricting legal paths.

No option has been selected or implemented by this report.
