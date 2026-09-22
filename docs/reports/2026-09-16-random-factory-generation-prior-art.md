# Prior-art review for random factory generation

## Decision

The subsequent [accepted dataset-generator
design](../designs/2026-09-16.02-recipe-sized-layout-dataset-generator.md)
specializes the direction below. Validated recipes and bounded facility-count
best matching now own semantic problem generation. Facility count and facility
packing are the two primary dataset controls; scalar layout-quality optimization
is intentionally omitted in favor of seeded diversity.

The current scale-aware procedural generator is a useful physical-layout stress
harness, but it is not an adequate random-factory generator or a sound source
distribution for training. Scaling its retry and A* budgets further will not fix
the underlying model.

The next architecture should declare a semantic production graph first, preserve
that graph as immutable routing obligations, optimize placement with a cheap
routability estimate, route all networks with negotiated congestion and bounded
rip-up, and only then legalize exact ports, crossings, splitters and convergers.
The existing constructive production planner, shared physical schema, route
kernel, validator, reports and survey runner should be reused. The accepted design
now defines the concrete implementation boundary.

No training distribution should be changed and no large corpus should be
generated from the current procedural layouts. The present implementation and
its 768-case survey remain valuable as a baseline against which a replacement can
be measured.

## What the current experiment established

The [scale-aware survey](2026-09-16-layout-generation-monte-carlo-survey.md)
established several useful engineering facts:

- external catalogs can be loaded once and shared across persistent workers;
- sequence pairs provide varied, nonoverlapping rectangle packings;
- the shared bounded A* kernel, crossing rules and independent validator work in
  a complete publication flow;
- fixed semantic requests, failures and resolved work limits can be recorded
  without retry-selection ambiguity; and
- the final baseline has one-attempt support through 100 blocks under its own
  abstract problem definition.

It did not establish realistic factories. The abstraction deliberately has no
recipe semantics. It samples facility types, samples a random sequence pair, and
then invents point-to-point edges only when they can be routed. A failed candidate
edge is silently replaced by another candidate. Consequently the output graph is
conditioned on the accidents of one placement and one routing order. Its
`commodity` values are identifiers rather than game items, and separate edges do
not model shared, capacity-carrying material networks.

The six visual inspections exposed the practical result: the profile controls
change spacing and edge count, but large layouts often send long paths around the
perimeter. These layouts are physically valid examples of the abstract schema;
they are not evidence of useful factory structure or human-like physical design.

## Failure diagnosis

The current generator makes three coupled irreversible choices:

1. [`packing.rs`](../../crates/aic-layout-gen/src/packing.rs) samples two random
   permutations and random gaps. The connection graph has no influence on this
   placement.
2. [`generate.rs`](../../crates/aic-layout-gen/src/generate.rs) samples a block
   pair and free port pair only after placement. An edge becomes part of the
   problem only if its local route succeeds.
3. Every accepted route is added to the blocked-cell set permanently. Later nets
   cannot ask an early net to move, and the placement cannot react to congestion.

This produces two selection effects. First, the generator does not sample a
problem and solve it; it samples from the subset of edges that happen to be easy
under the current partial solution. Second, early routes receive routing resources
for free while late routes inherit all their obstacles. More route expansions
explore the same compromised state more thoroughly. They do not restore paths
removed by earlier commitments or create clearance omitted during placement.

The final survey is consistent with that diagnosis. Of 146 raw failures, 117
exhausted route work, and doubling work alone in controlled 100-block trials did
not remove the cliff. Increasing port trials from eight to twelve made the same
budget less effective. Those observations are evidence against treating work
limits as the primary defect.

## What adjacent fields have already learned

### Placement must see the netlist and approximate routability

The original sequence-pair work uses the representation inside an optimization
loop; a sequence pair is a compact description of a placement topology, not a
quality model by itself. The method applies sequence-pair moves in simulated
annealing and evaluates objectives such as area and wire length ([Murata et al.,
1998](https://www.cecs.uci.edu/~papers/compendium94-03/papers/1998/ispd98/pdffiles/07_1.pdf)).
Randomly drawing one pair and never mutating it omits the search that makes the
representation useful.

RUDY estimates routing demand by spreading each net's estimated wire area over
its pin bounding rectangle. Its placement loop moves modules and allocates space
in response to the resulting demand/supply map ([Spindler and Johannes,
2007](https://websrv.cecs.uci.edu/~papers/date07/PAPERS/2007/DATE07/PDFFILES/08.7_1.PDF)).
The important transfer is the feedback loop, not the exact electronics formula:
a cheap congestion proxy should reject or mutate a placement before exact routing
spends most of the generation budget.

This remains current practice in an open production-grade flow. OpenROAD's global
placer exposes a routability-driven mode, uses RUDY by default, and can invoke its
FastRoute global router for a more expensive congestion signal
([OpenROAD global placement documentation](https://openroad.readthedocs.io/en/latest/main/src/gpl/README.html)).
The direct implication for this project is that `compact`, `balanced` and
`spacious` cannot be implemented only as independent random gap ranges. Packing
must be optimized against the fixed network and the actual two transport layers.

### Multi-net routing needs reversible allocation

PathFinder begins from the exact weakness seen here: obstacle-avoiding shortest
paths alone leave nets unroutable and make success depend on route order. It
temporarily permits resource sharing, repeatedly rips up and reroutes nets, and
raises present and historical congestion costs until the nets negotiate a legal
allocation ([McMurchie and Ebeling,
1995](https://janders.eecg.utoronto.ca/1387_2015/readings/pathfinder.pdf)).
The transferable mechanism is a reversible global state with history costs. The
existing single-net A* can remain the inner path search.

Modern global routers use the same broad pattern at larger scale: fast initial
pattern routes, a capacity model, and iterative maze-based rip-up and reroute for
hard nets. CUGR additionally optimizes global guides for whether a detailed router
can realize them, rather than assuming a low-cost coarse path is sufficient
([Liu et al., 2020](https://cwpui.com/doc/c10.pdf)). OpenROAD's FastRoute interface
similarly exposes explicit congestion-removal iterations
([OpenROAD global routing documentation](https://openroad.readthedocs.io/en/latest/main/src/grt/README.html)).

Endfield differs from IC routing: belt and pipe layers are few, facility ports are
directional, same-layer sharing is generally illegal, and crossings and junctions
are explicit catalog objects. Those differences change the resource graph and
legalizer. They do not invalidate negotiated congestion; they make irreversible
first-come routing even more fragile.

### Global guidance and exact legalization are separate contracts

EDA flows separate global routing from detailed routing because a coarse path can
manage congestion without encoding every final design rule. CUGR explicitly
evaluates global guides through a detailed router. TritonRoute then performs the
detailed search and repair needed for a design-rule-clean result
([TritonRoute paper](https://vlsicad.ucsd.edu/Publications/Journals/j133.pdf)).

The analogous split here is:

- global routing allocates approximate corridors and crossing demand for every
  immutable commodity network; and
- detailed legalization chooses exact facility ports, line cells, legal bridge
  sites, splitter/converger definitions, rotations and capacity assignments.

The project's independent physical validator remains the publication authority.
A zero-overflow global assignment is promising evidence, not a publishable
factory.

### Factory-layout research supports joint placement and paths

Traditional unequal-area facility-layout work often minimizes flow-weighted
rectilinear distance and therefore does not directly solve this game's discrete
belt/pipe problem. The closest useful result is structural: independently placing
facilities and planning material paths can require costly replanning, while joint
models of facility placement, handling points and paths produce better solutions
([Friedrich et al.,
2019](https://doi.org/10.1016/j.cor.2018.11.002)). Work that explicitly integrates
aisle structure reaches the same conclusion for manufacturing layouts
([Pourvaziri et al.,
2021](https://doi.org/10.1016/j.ejor.2020.07.058)).

These models are useful confirmation that placement and transport space are one
problem. Their usual objectives and continuous or aisle-based geometry are too
different to import as the generator itself.

### Synthetic graph generation needs measured structural targets

Synthetic circuit research did not treat node and edge count as an adequate
distribution specification. It used hierarchy, Rent-like interconnection
complexity and net-degree distributions to reproduce properties of real designs,
and still reported residual differences such as excess redundancy
([Stroobandt et al.,
1999](https://www.cecs.uci.edu/~papers/compendium94-03/papers/1999/ispd99/pdffiles/2_0_1.pdf)).
The transferable lesson is to measure a reference population before declaring a
random topology profile representative.

Recent industrial graph generation also separates a plant-level flow graph from
station-level behavior and projects generated graphs back onto hard constraints
([von Plessing et al.,
2025](https://re.public.polimi.it/handle/11311/1299442)). That is evidence for
hierarchical semantic generation, but not a reason to adopt diffusion now. The
paper addresses learned process-graph structure; it does not replace this
project's exact physical placement, routing and catalog legalization.

## Recommended architecture

```text
semantic request
    -> production graph and commodity networks
    -> hierarchical process modules
    -> routability-aware placement search
    -> negotiated global corridor routing
    -> exact game-rule legalization and repair
    -> independent validation
    -> accepted sample + stage metrics
```

### 1. Select a recipe-derived semantic problem before geometry

Sample an exact facility-count target, then use bounded best-match search over
target recipes, rates and path-specific upstream expansion or external-input cuts
from external game data.
Derive facility instances, item-specific supply/demand terminals, shared commodity
networks, capacity requirements and external boundaries. Freeze these obligations
before placement. A failed physical solve remains a failed sample; it must not
replace difficult edges until an easier graph emerges.

The existing source-plan, throughput, facility-requirement and instance-wiring
pipeline is the production grammar. Learned graph generation is deferred unless a
future dataset need cannot be represented by this catalog-backed contract.

### 2. Decompose into process modules

Cluster the production graph around recipe stages and shared commodities. Build
small modules with explicit boundary terminals, then place and connect modules at
the next level. This reduces the search horizon while retaining meaningful
structure.

Reuse the existing constructive planner's process modules, commodity networks,
capacity accounting, catalog-backed split/merge components and transactional
validation. The accepted [constructive planner
design](../designs/2026-09-02.00-constructive-layout-planner.md) already requires
immediately valid partial factories, bounded local rollback and eventual
rip-up-and-reroute. The [network-aware frontier
design](../designs/2026-09-05.00-network-aware-constructive-frontier.md) already
models network reuse and physical branch components. Those solved semantics must
not be replaced by the procedural generator's independent random edges.

### 3. Search placements instead of drawing one

Keep sequence pairs as one possible placement state for moderate module counts.
Add mutations such as swap, rotate, translate, gap change and module reorder. Use
a staged objective:

1. overlap and boundary legality;
2. estimated horizontal and vertical congestion overflow per transport layer;
3. port escape pressure and junction/crossing feasibility;
4. estimated route length and turns; and
5. area and profile-specific spacing targets.

Use a cheap RUDY-like demand map for most mutations and periodically run a
bounded global router to correct proxy error. Preserve several diverse candidates
instead of one scalar-best placement so that the output distribution does not
collapse to a single visual style.

### 4. Route all networks through negotiated congestion

Represent each layer as a capacitated resource graph. During global routing,
allow temporary corridor overflow, route every network, update present and
historical costs, and rip up and reroute. Preserve one deterministic configuration
for comparable benchmarks, while allowing the generator to use a bounded
portfolio of net orders or placement candidates.

This stage should emit total overflow, maximum overflow, overused cells, iteration
count, wire estimate and unresolved networks. The ISPD global-routing contest
prioritized total overflow, then maximum overflow, and compared wire length and
runtime only after zero overflow
([ISPD 2008 routing evaluation](https://www.ispd.cc/contests/08/ispd08rc.html)).
The same lexicographic discipline prevents a short but impossible route plan from
looking good.

### 5. Legalize exact game geometry

Use global corridors as guides for the existing exact local route search. Select
ports and legal component sites jointly for the affected neighborhood. If a guide
cannot be legalized, return congestion and obstruction evidence to the global
router or placement search. Bound this repair loop and retain the best fully valid
candidate.

The current shared `aic-layout` format and validator are useful at this boundary,
but the production semantic network representation must remain authoritative.
Adapters should be internal and short-lived; a successful conversion must not
erase item, network, rate or shared-capacity identity.

## Reuse and replacement map

| Current asset | Decision | Reason |
| --- | --- | --- |
| External catalog loading and identity | Reuse | Stable game-data boundary already exists. |
| `aic-layout` geometry and independent validation | Reuse | It gives a neutral physical publication contract. |
| Bounded deterministic single-net A* | Reuse as detailed-search kernel | The defect is global allocation, not local shortest-path correctness. |
| Batch workers, seeded requests, failure rows and survey summaries | Reuse | They provide the right evidence and recovery discipline. |
| Sequence-pair representation | Reuse inside placement search | One random draw is insufficient; mutations and a routability objective are required. |
| Constructive process modules and commodity networks | Reuse and generalize | They already preserve recipe, item, capacity and junction semantics. |
| Random post-placement edge acceptance | Replace | It changes the problem to fit the partial solution. |
| Permanently committed sequential routes | Replace | It creates route-order bias and irreversible congestion. |
| Gap range as the packing model | Replace | Spacing must respond to network demand and requested style. |
| Facility count plus edge count as the distribution | Replace | It does not characterize semantic or hierarchical structure. |

## Bounded research sequence

### Gate A: define a reference panel

Before another generator rewrite, assemble a compact panel containing existing
valid production layouts, constructive outputs and deliberately hard synthetic
netlists. Record semantic and physical descriptors: facility types, recipe depth,
fan-in/out, shared-network size, module sizes, port pressure, route length, turns,
crossings, junctions, occupancy, aspect ratio and congestion heatmaps. If player
blueprints later become available, keep them as a separate held-out reference
population with explicit provenance.

Exit condition: the team can state which target statistics the generator should
cover, and which are unavailable rather than filling them with guesses.

### Gate B: prove global routing on fixed placements

Freeze the exact same facility poses, ports and immutable netlists for both
routers. Compare the current sequential A* baseline with a minimal negotiated
congestion prototype that reuses the A* kernel. Start with one transport layer and
no junction synthesis, then add the actual layer/component rules.

Primary metrics are completion, total/max overflow before legalization, legal
completion after legalization, route work and wall time. Route length is secondary
until completion is equal.

Exit condition: rip-up materially improves legal completion on a controlled hard
panel without a prohibitive integrated runtime regression.

### Gate C: add placement feedback

For fixed semantic netlists, compare random sequence pairs with mutated
sequence-pair search using the cheap congestion proxy and periodic global-route
feedback. Hold the router and resource budget fixed.

Exit condition: placement search improves zero-overflow or legally completed
yield across the hard panel, rather than only reducing estimated wire length.

### Gate D: connect semantic generation

Feed recipe-derived immutable problems and existing process modules into the
placement/router pipeline. Do not start with 100 facilities. Grow through module
count and network complexity only when the previous stratum has adequate raw
yield and bounded cost.

Exit condition: generated samples satisfy production semantics and physical
validation, cover the declared reference descriptors, and retain failures without
substitution.

### Gate E: evaluate training value

Only after Gates A-D should generated layouts enter pretraining. Freeze a corpus
version and compare it with the current source distribution on held-out real or
constructive tasks. Keep generation validity, geometric diversity and downstream
learning benefit as separate claims.

## Approaches to defer

- **More retries and larger A* budgets:** useful for calibrating a fixed
  architecture, but unable to repair placement blindness or route-order bias.
- **A global exact joint solver:** retained for small references and local oracles;
  prior repository evidence already rejected it as the target-scale production
  generator.
- **Pure templates:** useful seeds and regression fixtures, but insufficient as
  the distribution by themselves.
- **Diffusion or reinforcement learning as the first replacement:** both need a
  trustworthy objective, hard-constraint projection, reference distribution and
  deterministic evaluator. Adding them now would learn or optimize the present
  proxy defects.
- **Imitating visual appearance alone:** screenshots cannot verify commodity
  identity, rates, shared capacity, reachable ports or legal component topology.

## Superseded immediate decision

The accepted design makes the recipe best-match selector the first slice. A
research-only negotiated-routing comparison around the existing resource graph
and A* kernel follows once it has frozen realistic semantic inputs.

The scale-aware procedural branch should remain uncommitted as a reviewed
baseline until this direction is accepted. Its semantic request/reporting layer,
batch harness and survey evidence are likely reusable; its placement and
connection-construction core should not be promoted as the production generator.
