# AIC Game Rules Mathematical Model

## Purpose and authority

This document gives a mathematical contract for the spatial Automated Industry Complex (AIC)
model. Its primary audience is an independent reviewer of exact propagators: a reviewer should be
able to decide whether a proposed propagation rule removes only values that cannot occur in a
legal solution.

The model is descriptive, not a new implementation proposal. When sources disagree, current code
and accepted later contracts are described explicitly instead of silently reconciling older design
documents.

Every rule is labelled with one of these statuses:

- **Confirmed current semantics**: encoded by the current authoritative exact solve path and/or
  checked by independent witness validation.
- **Approved semantics, incomplete encoding**: accepted repository policy or a later accepted
  contract, but not completely represented by the current solver and validator.
- **Open ambiguity**: not established well enough by source data, code, or an accepted decision to
  justify correctness-pruning propagation.

The authoritative execution path at the time of writing is
`layouts::integrated::solve_integrated_layout` -> the iterative SCC harness ->
`exact::shared_layer::solve_factored_endpoints_with_prior`. The current public witness schema is
`INTEGRATED_LAYOUT_SCHEMA_VERSION = 20`.

## Evidence map

The following are the concrete evidence sources cited by later rules.

| Reference | Evidence |
| --- | --- |
| E1 | `AGENTS.md`, especially Exact Solver And Heuristic Policy, Exact Search Research Policy, and Layout Bound Semantics |
| E2 | `crates/aic-data/src/facilities.rs`: facility schema, port schema, edge validation, allowed rotations |
| E3 | `crates/aic-data/src/layouts/integrated/geometry.rs`: rotated port position and outside-adjacent connection cell |
| E4 | `crates/aic-data/src/layouts/integrated/model.rs`: validated model preparation, endpoint roles, rate-to-lane splitting, and hard request ceilings |
| E5 | `crates/aic-data/src/layouts/integrated/networks.rs`: item/transport network normalization and exact integer flow scaling |
| E6 | `crates/aic-data/src/layouts/integrated/exact/shared_layer.rs`: current placement, terminal, layer, flow, item, topology, collision, and objective constraints |
| E7 | `crates/aic-data/src/layouts/integrated/exact/boundary_terminals.rs`: actual-used-boundary terminal domain and witness check |
| E8 | `crates/aic-data/src/layouts/integrated/witness.rs`: independent placement, network, capacity, topology, crossing, and used-bounds validation |
| E9 | `crates/aic-data/src/layouts/integrated/exact/objective.rs` and `crates/aic-data/src/layouts/integrated/score.rs`: exact lexicographic stages and report-side scoring |
| E10 | `crates/aic-data/src/layouts/growth.rs` and `crates/aic-data/src/layouts/integrated/harness/iterative_scc.rs`: SCC construction, output-first cumulative growth, frontier projection, and non-binding hints |
| E11 | `crates/aic-data/src/recipes/instances.rs` and `crates/aic-data/src/layouts/integrated/model.rs::prepare_endpoint`: facility, external, target, surplus, and projected frontier endpoint contracts |
| E12 | `crates/aic-data/src/logistics.rs`, `logistics/transport.rs`, and `logistics/components.rs`: item transport kinds, line capacity schema, and component schema validation |
| E13 | `data/game/normalized/{README.md,items.json,transports.json,facilities.json,logistics-components.json}`: current external runtime game data |
| E14 | `docs/designs/shared-boundary-terminal-cutover.md`: accepted shared external-terminal semantics |
| E15 | `docs/designs/2026-08-31.02-circulation-permitted-routing.md`: accepted circulation semantics |
| E16 | `docs/designs/2026-08-30.32-exact-joint-incremental-solver.md`: accepted joint-model and cumulative exact-harness contract |
| E17 | `docs/designs/2026-08-30.16-logistics-component-source-data.md`: provenance and known limits of component geometry/topology data |

Earlier route-per-requirement and constructive-router documents are historical evidence only where
later accepted contracts and current code have not superseded them. In particular,
`2026-08-30.06-integrated-placement-routing.md`, `2026-08-30.12-multi-edge-integrated-routing.md`,
and the independent-route paragraph in `2026-08-30.15-transport-capacity-catalog.md` do not
describe the current shared commodity-network formulation.

## Coordinate system, sets, and indices

For one solve request, let:

- `W = max_width` and `H = max_height` be positive caller-supplied hard ceilings;
- `X = {0, ..., W - 1}`, `Y = {0, ..., H - 1}`, and `G = X x Y` be the finite search grid;
- `D = {N, E, S, W}` be cardinal directions;
- `delta(N) = (0,-1)`, `delta(E) = (1,0)`, `delta(S) = (0,1)`, and
  `delta(W) = (-1,0)`;
- `T = {belt, pipe}` be transport layers;
- `I` be item IDs from the validated item catalog;
- `F` be production-facility instances in the prepared wiring graph;
- `K` be capacity-split logical material requirements;
- `N = {(tau(i), i) | i occurs in K}` be physical commodity networks, one per item and
  its fixed transport kind;
- `A = {(u,v) in G x G | u and v are orthogonally adjacent}` be directed grid arcs.

Coordinates use `(0,0)` at the north-west. Positive `x` points east and positive `y` points south.
The four rotations are clockwise degrees `Q = {0, 90, 180, 270}`.

**Confirmed current semantics.** `W` and `H` define the candidate domain only. They are not a
required output size, a default, an official whole-factory limit, or unused blueprint geometry.
The model rejects non-positive ceilings and values outside its checked integer domain. Game
blueprint export limits are separate external data and are not solver ceilings. Evidence: E1, E4,
`crates/aic-data/src/layouts/placement.rs::validate_facility_placement_request`, and
`data/game/normalized/blueprint-limits.json` as explained by
`docs/designs/2026-08-30.26-official-blueprint-limits.md`.

**Plain-English interpretation.** The caller gives a rectangle in which the solver may search.
The installed factory is only the cells actually used inside that rectangle.

## Facilities, rotations, and occupied footprints

For facility instance `f in F`, external data supplies:

- canonical footprint width `w_f > 0` and height `h_f > 0`;
- allowed rotations `Q_f`, a non-empty subset of `Q`;
- a set of directional ports `P_f`.

For rotation `r`, the rotated dimensions are

```text
(w_f(r), h_f(r)) =
  (w_f, h_f)  if r is 0 or 180,
  (h_f, w_f)  if r is 90 or 270.
```

For every legal placement candidate `c = (f,r,x,y)`, define

```text
Foot(c) = {(x + dx, y + dy) | 0 <= dx < w_f(r), 0 <= dy < h_f(r)}.
```

Only candidates satisfying `Foot(c) subseteq G` exist. Let `z_c in {0,1}` select a candidate:

```text
for every f:  sum_{c belongs to f} z_c = 1
for every g in G: sum_{c: g in Foot(c)} z_c <= 1.
```

The current factored implementation represents the first equality with one finite-domain choice
variable per facility and channels every footprint-cell occupancy from that choice. The aggregate
occupancy variable has domain `{0,1}` and equals the sum of per-instance occupancies, which enforces
the second inequality.

**Confirmed current semantics.** A facility chooses exactly one allowed rotation and in-ceiling
origin; complete rectangular footprints cannot overlap. Rotation remains a distinct decision even
when two rotations have equal dimensions. Evidence: E2, E6 `build_placements`, E8
`validate_placements`, and the normalized facility examples in E13.

**Plain-English interpretation.** Every machine occupies every cell of its rotated rectangle, and
two machines cannot occupy the same ground cell.

## Facility ports and one-cell connection clearance

Each port `p in P_f` has a local boundary cell `pos_p = (a_p,b_p)`, direction
`dir_p in {input, output}`, transport `tau_p in T`, and outward edge `edge_p in D`. Catalog
validation requires `pos_p` to lie inside the canonical footprint and on the named edge.

Clockwise rotation transforms the local port cell as follows:

```text
rho_0(a,b)   = (a,b)
rho_90(a,b)  = (h_f - 1 - b, a)
rho_180(a,b) = (w_f - 1 - a, h_f - 1 - b)
rho_270(a,b) = (b, w_f - 1 - a).
```

The edge is rotated by the same number of quarter turns. For selected placement
`c = (f,r,x,y)`, define

```text
port_cell(c,p) = (x,y) + rho_r(pos_p)
connection(c,p) = port_cell(c,p) + delta(rotate_r(edge_p)).
```

`port_cell(c,p)` belongs to the facility footprint. `connection(c,p)` is the one cell immediately
outside the facility. A facility terminal may select `(c,p)` only if `connection(c,p) in G`.
The selected connection cell is a transport cell, so it is excluded from every facility footprint
by the transport/facility collision rule below.

For terminal `e` and its compatible option set `O_e`, let `q_eo in {0,1}`:

```text
sum_{o in O_e} q_eo = 1
q_e,(c,p) <= z_c.
```

The factored encoding expresses the same relation through placement choice, port choice, a table
of rotated geometry keys, and an element constraint.

**Confirmed current semantics.** The facility endpoint of a supply requirement selects an output
port; the facility endpoint of a demand requirement selects an input port. In both cases the port
transport must equal the item's fixed transport. Port direction points across the facility
boundary; the transport arm at the outside connection cell points back toward the facility for a
facility endpoint. Evidence: E2-E6 and E8 `validate_terminal_endpoint`.

**Confirmed current semantics.** Clearance is required for every *selected* port connection, not
for every unused catalog port. A placement flush with a search edge remains legal if no selected
terminal needs a port facing through that edge. This is the behavior of E3 and the factored
selector in E6; it supersedes the all-ports boundary clearance described in the older
`2026-08-30.05-port-aware-placement-boundaries.md`.

**Plain-English interpretation.** A machine port is painted on one occupied edge cell of the
machine. A connected belt or pipe starts on the adjacent free cell outside that edge. The solver
must leave that first connected cell available, but it need not reserve space for a port that the
layout does not use.

**Open ambiguity.** The catalogs do not attach an item ID or recipe slot ID to a port. The current
model therefore allows any belt item through any direction-compatible belt port, and likewise for
pipe items. Item filters, control-port behavior, and any requirement that unused ports remain
clear need additional source data and a contract before a propagator may assume them. Evidence:
E2, E13, and E17.

## Items, belt and pipe layers, and physical collision

The item catalog defines a total function

```text
tau: I -> T.
```

It is input data, not a solver decision. Current normalized source mapping assigns source phase
type `1` to belt and phase types `2` and `4` to pipe. The transport catalog supplies a positive
rational line capacity

```text
C_line(t) = quantity(t) * 1000 / duration_ms(t)
```

in items per second. Current normalized values are `1/2` item/s for belt and `2` items/s for pipe,
but the solver loads them on every execution and does not compile those values into code.

For each `t in T` and cell `g`, let `L_tg in {0,1}` mean that the physical transport layer is
occupied. Let `O_g in {0,1}` mean that a production facility occupies `g`. The hard collision rule
is

```text
O_g + L_tg <= 1                    for every g and t.
```

There is deliberately no constraint `L_belt,g + L_pipe,g <= 1`.

**Confirmed current semantics.** Facilities exclude both transport layers. One belt tile and one
pipe tile may coexist at the same `(x,y)` because the two layers are independent. Multiple
commodity flows on one transport layer share one physical occupancy state and must satisfy the
item/topology rules below. Evidence: E6 `build_transport_occupancy`, E8, E12, and E13.

**Plain-English interpretation.** A machine blocks both kinds of logistics at its ground cell.
Belt and pipe are modeled as separate vertical systems, so one of each may use the same map
coordinate. Two unrelated belt constructions cannot simply stack in one belt layer; they must form
one legal line, branch, convergence, or bridge topology.

**Open ambiguity.** The repository's accepted solver contract permits belt/pipe coordinate
coexistence, but the normalized source tables do not by themselves prove every in-game vertical
clearance interaction, especially when a belt component and a pipe component share a coordinate.
Do not add cross-layer exclusion or additional coexistence without representative game evidence or
an explicit contract change.

## Logical requirements and fungible material networks

Each prepared logical requirement `k in K` has source node `s_k`, target node `d_k`, item `i_k`,
positive exact rate `R_k`, and provenance ID. Before network normalization, any logical edge rate
above `C_line(tau(i_k))` is split exactly into the minimum number of positive lane requirements:

```text
number of lanes = ceil(R_k / C_line(tau(i_k)))
0 < R_lane <= C_line(tau(i_k))
sum R_lane = R_k.
```

Every lane retains the original item, source, target, and diagnostic provenance. Physical routing
then groups all lanes with equal `(tau(i), i)` into one commodity network `n`. The requirement IDs
remain in the witness, but they do not prescribe source-to-target path pairing.

For network `n`, let `S_n` be its supply terminals and `D_n` its demand terminals. Preparation
requires

```text
sum_{e in S_n} R_e = sum_{e in D_n} R_e.
```

**Confirmed current semantics.** Same-item requirements form a solver-selected material network.
They may share trunks, split, converge, and serve a demand from any compatible supply in the same
network. A placement-candidate router that preserves individual logical pairs is not equivalent to
this model. Evidence: E1, E4, E5, E6, E8, and E16.

**Plain-English interpretation.** The wiring graph says how much of an item must enter and leave
each machine, not which individual ore particle must follow which dedicated belt. Once identical
material enters the factory network, the solver may combine and redistribute it legally.

**Approved semantics, incomplete validation.** Capacity-split lanes remain independent terminals
with stable requirement-derived IDs even when they share a node, direction, item, and rate. The
current exact model creates one terminal decision per lane, but independent witness validation
currently aggregates expected and observed terminal rates by `(node, direction)`. That aggregation
can fail to detect a forged witness that changes the number or identity of same-node lane terminals
while preserving their total rate. Propagator proofs may rely on the model's independent terminal
variables, but an accepted production witness must not be treated as independently validated for
lane identity until the validator keys expected terminals by terminal ID.

## Exact integer flow units

Rates are rational. For each network `n`, choose a checked positive scale `sigma_n` equal to the
least common multiple of the denominators of its terminal rates, line capacity, and relevant
component capacities. Convert every rate `r` to integer units

```text
U_n(r) = r * sigma_n.
```

The conversion must be exact and fit the solver's 32-bit flow domain. Let `C_n` be the resulting
line-capacity units and `U_e` a terminal's exact units.

For each directed physical arc `a in A` on transport layer `t`, the shared-layer formulation has
an activation `y_ta in {0,1}`, a positive-flow variable `f_ta`, and an item identity selected on
the incident arms. Conditional on arc item `n`:

```text
y_ta <= f_ta <= C_n * y_ta.
```

The implementation allocates the scalar flow variable to the maximum unit capacity among items on
that layer, then posts the item-conditional `C_n` bound. Opposite directed uses of the same
physical side are mutually exclusive.

**Confirmed current semantics.** Selected geometry always carries positive flow; unselected arcs
carry zero. Capacity and all terminal rates use exact integer scaling, not floating-point
approximations. Evidence: E5, E6 `grid_arcs` and `build_layer`, and E8 segment validation.

**Plain-English interpretation.** A drawn belt or pipe segment must actually carry material, and no
segment may carry more than one catalog line. Rational rates are multiplied into exact whole-number
solver units before constraints are posted.

## Terminal selection, direction, and boundary terminals

Every lane contributes one supply terminal and one demand terminal to its commodity network. Each
terminal selects exactly one legal geometry option.

Facility terminal options are the selected port connection cells described above. External nodes
use a different option set. Let the actual used bounds be `B_w` and `B_h`. An external terminal
chooses a cell `g = (x,y)` and outward side `d` satisfying exactly one of

```text
d = N and y = 0
d = W and x = 0
d = E and x + 1 = B_w
d = S and y + 1 = B_h.
```

The boundary terminal cell is an ordinary occupied cell of its shared commodity network. Corner
cells may select either incident outward side. External inputs inject exact flow into the network;
target and surplus outputs consume exact flow from it.

**Confirmed current semantics.** Every prepared external endpoint selects one terminal on the
boundary of the *actual used bounding box*, not the caller's `W x H` search perimeter. External
terminal geometry participates in ordinary occupancy, item assignment, direction, conservation,
capacity, and objectives. Evidence: E6 `build_factored_terminals`, E7, E8, E11, and E14.

**Plain-English interpretation.** An import or export is an ordinary belt/pipe end whose other side
continues outside the generated factory. The solver chooses where that end reaches the compact
factory boundary.

**Open ambiguity.** The current shared-layer model permits multiple logical terminals of the same
commodity and direction to select the same physical `(cell, outward side)`: terminal presence is
the Boolean OR of those selections and their exact flows are summed. No accepted game rule or
source record establishes whether this represents one legal shared boundary manifold or whether
each logical terminal requires a distinct physical boundary connector. A propagator must preserve
the current co-location behavior until this contract is explicitly decided.

## External input and final output contract

The prepared wiring graph distinguishes four node kinds:

- `Facility`: a placed producer and/or consumer;
- `External`: a true input or a temporary SCC-frontier input, valid only as a route source;
- `Target`: the requested final product, valid only as a route target;
- `Surplus`: required steady-state by-product disposal/output, valid only as a route target.

For every external-like endpoint, the node's item must equal the logical edge's item. An
external-to-external requirement is invalid because at least one endpoint of every routed
requirement must be a production facility. In physical network terminology:

```text
External source -> supply terminal -> shared network
shared network -> demand terminal -> Target or Surplus output.
```

The selected target rate and surplus rates are fixed upstream. Recipe choice, throughput, and
facility counts are not layout-solver decisions.

**Confirmed current semantics.** Direction and item-role validation occur before exact model
construction. The final requested product and all modeled surplus leave through ordinary selected
used-boundary terminals. Evidence: E4 `prepare_endpoint`, E11, E14, and the target/surplus
construction in `crates/aic-data/src/recipes/wiring.rs`.

**Plain-English interpretation.** Raw materials may enter the generated layout; the requested
product and unavoidable by-products may leave it. The layout solver decides only where and how
those already-calculated flows cross the generated factory boundary.

**Open ambiguity.** The model contains no geometry for outside depots, storage, bootstrap
inventory, disposal buildings, or boundary manifold sharing beyond the selected terminal cell.
Those systems must not be inferred by a propagator.

## Per-cell item assignment and flow conservation

For each transport layer and cell side, define incoming arm `a^-_tgd`, outgoing arm `a^+_tgd`, and
item code `m_tgd`, where `0` means no arm. The shared-layer table enforces:

```text
(a^-_tgd, a^+_tgd, m_tgd) is either
  (0,0,0), (1,0,item), or (0,1,item)
```

for some network item on layer `t`. Thus one side cannot be simultaneously incoming and outgoing.
Selected adjacent arcs equate item codes on their two incident sides. Selected terminals force
their network's item code.

At a non-bridge cell, every active arm has the same non-zero item code. For every cell, including
terminal and component cells, scalar flow conservation is

```text
sum incoming segment flow + sum selected supply-terminal units
  = sum outgoing segment flow + sum selected demand-terminal units.
```

Line capacity is enforced independently on every directional arm after conditioning on its item.

**Confirmed current semantics.** Material is neither created nor destroyed by spatial routing.
Every selected terminal contributes its exact fixed amount, and different items cannot merge at a
plain, splitter, or converger cell. Evidence: E6 `build_layer` and `post_cell_topology`, E8
`validate_network`, and E5.

**Plain-English interpretation.** Whatever enters a normal belt/pipe cell must leave it, except for
the exact amount inserted or removed by a terminal. A physical arm has one material identity.

## Plain cells, splitters, and convergers

Let `deg^-(g)` and `deg^+(g)` count active incoming and outgoing arms on one transport layer at a
cell. Without a selected branch component or bridge:

```text
deg^-(g) <= 1 and deg^+(g) <= 1.
```

For a selected splitter rotation:

```text
deg^-(g) = 1
2 <= deg^+(g) <= 3
the incoming direction and every outgoing direction belong to the rotated catalog pattern
total incoming flow = total outgoing flow
total incoming flow <= C_splitter(t).
```

For a selected converger rotation:

```text
2 <= deg^-(g) <= 3
deg^+(g) = 1
every incoming direction and the outgoing direction belong to the rotated catalog pattern
total incoming flow = total outgoing flow
total incoming flow <= C_converger(t).
```

At most one splitter, converger, or bridge is selected at a cell within one transport layer.
Component definitions and allowed rotations come from the validated external catalog. Current
normalized components occupy one horizontal cell and expose `1 -> 3` or `3 -> 1` maximum
direction patterns.

**Confirmed current semantics.** The solver chooses component kind, cell, and rotation jointly
with placement and routing. It may use either two or three branch arms within a catalog maximum;
it does not insert components after solving. Evidence: E6 `post_cell_topology`, E8
`validate_branch_topology`, E12, E13, E16, and E17.

**Plain-English interpretation.** Ordinary line cells do not fork. A splitter or converger is
required whenever one line becomes multiple lines or multiple lines become one, and the installed
component must face the used directions and carry no more than its catalog capacity.

**Open ambiguity.** Current constraints conserve total flow but do not require equal flow across
splitter outputs. E17 notes equal-split behavior as a material-flow concern, while the normalized
catalog stores topology and aggregate capacity only. Whether the game requires equal split,
priority, filtering, or configurable ratios must be resolved before an equality-based splitter
propagator is sound.

## Logistics bridges and same-layer crossings

A selected bridge at cell `g` requires one active incoming and one active outgoing segment on each
axis. The channels are straight and independent:

```text
flow(W -> g) = flow(g -> E)
item(W -> g) = item(g -> E)
flow(N -> g) = flow(g -> S)
item(N -> g) = item(g -> S).
```

Equivalent reverse travel is represented by choosing the opposite directed arcs. Each axis flow
is bounded by the external bridge capacity. The horizontal and vertical item IDs may differ. A
bridge cell cannot also host a facility terminal or branch component.

**Confirmed current semantics.** A same-layer perpendicular crossing requires a selected bridge;
the two axes do not merge. Belt and pipe each have their own bridge capability and may independently
occupy the same map coordinate because layers are separate. Evidence: E6
`post_cell_topology`, E8 `validate_crossings`, E12, E13, and E17.

**Plain-English interpretation.** A logistics bridge lets one belt or pipe channel pass straight
over another channel of the same transport type without mixing their materials.

**Open ambiguity.** Source records establish one-cell horizontal footprint, all four connector
directions, capacity, and localized bridge identity, but not the full operational semantics of
elevation, rotation relevance, mixed-item crossing, construction adjacency, or any entrance ramp.
The current model treats bridge rotations as selectable output identity while all rotations have
the same four-direction behavior. Stronger bridge propagation needs additional game evidence.

## Selected terminals and connectivity

For every terminal `e`, exactly one geometry option is selected and its exact `U_e` is inserted or
removed at that cell. Global conservation and non-negative flows imply that every positive supply
in a connected flow component is balanced by positive demand in that component, and vice versa.
They do not impose the original logical source-to-target pairing.

The authoritative production solve currently posts no separate reachability tree, topological
order, or terminal-rooted connectivity witness. Consequently the legal set includes:

- more than one disconnected, locally balanced component of the same item network;
- a shared trunk connecting arbitrary compatible supplies and demands;
- an additional disconnected net-zero circulation.

All selected arcs in any such component still carry positive flow and consume line/component
capacity and objective geometry.

**Confirmed current semantics.** Terminal balance and physical directed flow are mandatory;
terminal-to-terminal path pairing and global connectedness of all occupied cells are not. Net-zero
circulation is explicitly permitted. Evidence: E5, E6 default `ConnectivityMode::None`, E8, E15,
and E1. Any older design acceptance criterion that requires all selected route cells to be
terminal-reachable is superseded by the later circulation-permitted cutover E15.

**Plain-English interpretation.** Every machine still receives and emits the required amount, but
identical material may be delivered through several separate subnetworks. The solver is also
allowed to include a closed loop carrying material, although that loop consumes space and
capacity.

**Approved semantics, incomplete encoding.** Research-only possible-graph and connectivity-witness
propagators exist in E6, but they are not part of the authoritative default solve. A future
production propagator may enforce only a property proven redundant with the circulation-permitted
flow contract. Requiring every active cell to be terminal-reachable would be a semantic cut and is
not presently sound.

## Cycles

Two different cycle concepts must not be conflated.

### Production cycles

A production cycle is a directed cycle among facility-instance wiring nodes. Upstream throughput
must already have found a feasible steady state and fixed every edge rate. The spatial solver
receives those fixed rates and treats the cycle's edges like all other material requirements.

**Confirmed current semantics.** Production cycles are legal; recipe rates, bootstrap inventory,
and startup schedules are outside the spatial model. Evidence: E10, E11, E16, and
`docs/designs/2026-08-30.11-cyclic-production-steady-state.md`.

### Route circulation

A route circulation is a positive directed flow cycle whose net supply and demand are zero. It is
legal under the current spatial model. It cannot create material, but it consumes capacity and
used geometry. There is no cycle cleanup or route-arc objective after solving.

**Confirmed current semantics.** Evidence: E6, E8, and the accepted cutover E15.

**Plain-English interpretation.** A recipe loop means machines depend on one another at steady
state. A route loop means material physically circles through logistics tiles. Both are allowed,
but for different reasons and at different model layers.

## Used bounding box and hard ceilings

Define cell use

```text
U_g = O_g OR L_belt,g OR L_pipe,g.
```

Logistics components necessarily activate their transport cell, so no separate component-only
geometry is needed. Translation symmetry is removed exactly by requiring used geometry to touch
the north and west search edges:

```text
exists g=(0,y): U_g = 1
exists g=(x,0): U_g = 1.
```

Then

```text
B_w = max_{g=(x,y): U_g=1} (x + 1)
B_h = max_{g=(x,y): U_g=1} (y + 1)
A_used = B_w * B_h
1 <= B_w <= W, 1 <= B_h <= H.
```

The witness validator independently recomputes minimum coordinates and used dimensions from
placements, network cells, and components and requires minimum `(0,0)` and reported bounds
`(B_w,B_h)`.

**Confirmed current semantics.** Unused search capacity is not reported footprint. Belt and pipe
at the same coordinate contribute one used coordinate to the bounding box but two physical
transport tiles to the second objective. Evidence: E1, E6 `build_objectives`, E7-E9, and E16.

**Plain-English interpretation.** The blueprint rectangle wraps tightly around everything the
solver installed. Empty rows and columns in the caller's larger search canvas do not count.

## Exact objective ordering

The authoritative solver performs proof-gated sequential minimization in this exact order:

```text
1. A_used                    used bounding-box area
2. sum_g (L_belt,g + L_pipe,g)
                             physical belt/pipe tile count
3. total_route_turns         plain 1-in/1-out orthogonal turns
4. max(B_w, B_h)             maximum used side
5. selected logistics component count
```

After a stage is proven optimal, its value is fixed before the next stage. If the time limit
expires, an independently validated incumbent may be returned as `feasible`; an unproven result is
not reported as infeasible. In the current shared-layer path, a prior exact solution supplies
placement warm-start values only and does not add an objective or a hard constraint.

**Confirmed current semantics.** The five fields above are the complete exact objective currently
optimized by `optimise_lexicographically`. Evidence: E6, E9, and E1.

**Plain-English interpretation.** The solver first makes the factory's enclosing rectangle as
small as possible. Only among equally small rectangles does it shorten physical logistics, then
reduce bends, then prefer a less elongated shape, then use fewer special components.

**Approved semantics, incomplete encoding.** E16 describes later stability tie-breakers (moved
prior facilities, Manhattan displacement, and rotation changes), and `LayoutScore` can calculate
them, but the current exact optimizer does not include them. Prior-phase geometry is currently a
non-binding hint. A propagator must not prune on those stability fields as though they were proven
objective bounds.

## Production graph and cumulative SCC growth boundary

Let `P = (F,E_F)` be the directed graph containing only facility-to-facility wiring edges. The
growth planner computes its strongly connected components `C_1, ..., C_j` and the condensation
DAG. An SCC is atomic: all of its facilities enter the same phase. Growth is output-first. A
component is ready only when every downstream component is already included.

The current harness asks for at most one ready SCC per phase; an SCC larger than the nominal
one-facility limit still enters atomically. Phase `p` solves the complete cumulative subgraph
induced by included SCCs. If a required upstream facility is not yet included, its edge is
projected to a synthetic `FrontierExternal` supply terminal while preserving item, rate, edge ID,
and missing-facility provenance.

For each cumulative phase:

```text
legal phase variables = all legal placements, rotations, selected ports,
                        boundary terminals, routes, flows, and components
                        for that cumulative graph.
```

The previous complete phase solution currently contributes only non-binding placement warm-start
values. It may not restrict the enlarged phase. The final phase is the original full wiring graph
with no synthetic frontier.

**Confirmed current semantics.** SCC growth is an experimental harness boundary, not a game rule
and not a decomposition of the final legal solution set. A phase timeout returns complete prior
phase history and `unknown`/`feasible` evidence as applicable; it does not trigger a heuristic
fallback. Evidence: E1, E10, and E16.

**Approved semantics, incomplete encoding.** E16 approves translating stable prior ports, routes,
flows, and components into additional non-binding hints. E6 currently calls
`build_placement_solver_hint`, so those broader hints are not present in the authoritative
shared-layer path. Their absence affects search guidance, not feasibility or objective semantics.

**Plain-English interpretation.** To study search growth, the tool first solves the output-side
machines and gradually adds upstream machine cycles. Each step may rearrange everything already
placed. The complete final factory still has to satisfy one full joint exact model.

## Explicit invariant checklist for propagator review

A proposed propagator for the authoritative exact model must preserve every assignment satisfying
all confirmed invariants below.

1. Every facility chooses one allowed rotated rectangle fully inside the request ceilings.
2. Production-facility rectangles do not overlap.
3. Every selected facility terminal chooses one direction- and transport-compatible port linked to
   the selected placement and an in-grid outside-adjacent connection cell.
4. Every item uses its catalog-fixed transport layer.
5. Every logical lane rate is positive, no larger than one line capacity, and preserved exactly in
   its item network.
6. Every network has equal total prepared supply and demand.
7. Every terminal selects exactly one legal facility or used-boundary geometry and injects or
   consumes its exact integer-scaled flow.
8. Every selected directed arc has positive flow, every unselected arc has zero flow, and every
   active arm respects its item-conditional capacity.
9. Every cell conserves total flow after terminal supply and demand are included.
10. A non-bridge transport cell carries only one item; the two bridge axes may carry different
    items without mixing.
11. Plain topology is at most one incoming and one outgoing arm. Higher legal degree requires a
    catalog-valid selected splitter, converger, or bridge.
12. Splitter, converger, and bridge topology, rotation constraints, and capacity constraints hold.
13. A facility cell cannot contain belt or pipe occupancy. Belt and pipe may coexist with each
    other at one coordinate.
14. Same-item physical flow may share trunks and may be disconnected into locally balanced
    subnetworks; original logical source-target pairing is not preserved physically.
15. Positive net-zero route circulation is legal. Zero-flow active geometry is illegal.
16. External inputs are supplies; target and surplus outputs are demands; their item IDs must
    match the containing network.
17. Every external terminal lies on its selected side of the actual used bounding box.
18. Used geometry is canonically translated to minimum `(0,0)` and reported bounds are exact used
    bounds, never the request ceilings.
19. Objective pruning follows the five current lexicographic stages and may use a later-stage bound
    only after all earlier stage values are fixed/proven for that search.
20. SCC phases and prior solutions do not constrain the legal placement-routing set of the final
    full graph.

The following are specifically *not* safe assumptions without a new accepted contract and
evidence:

- every catalog port, including unused ports, must have a free outside cell;
- a belt/pipe port is dedicated to one item or recipe slot;
- one terminal or one logical edge owns one physical route;
- every same-item network must be globally connected;
- every active cell must be reachable from a terminal;
- routes must be acyclic;
- splitter outputs are equal, prioritized, or filtered;
- belt and pipe cannot share a coordinate;
- a preferred corridor, side, port, coordinate window, or route order is complete;
- an earlier SCC phase fixes any later coordinate, rotation, port, route, or component;
- request ceilings are output dimensions or game-wide factory limits.

## Unresolved questions requiring domain decisions or game evidence

1. Must an unused facility port's outside-adjacent cell remain in bounds and unoccupied, or is
   selected-port clearance sufficient?
2. Are ports item-specific, recipe-slot-specific, filtered, or capacity-limited independently of
   the attached line? Can multiple same-item terminals legally aggregate through one port while
   their total stays within line capacity?
3. May multiple logical external terminals share one physical boundary `(cell, outward side)`, or
   does each require a distinct connector tile/side?
4. Do game splitters divide equally, and if so under what demand/back-pressure conditions? Are
   priority and filter modes part of the target scope?
5. Does a logistics bridge permit two different items, both travel directions, and every reported
   rotation without additional entrance, elevation, or neighboring-cell constraints?
6. Are all belt/pipe and belt-component/pipe-component coordinate overlaps physically legal in
   game, or are there cross-layer exceptions absent from current source normalization?
7. Do target and surplus outputs require concrete depot/storage/disposal geometry outside the
   current used-boundary terminal, and are multiple boundary terminals allowed to share an outside
   manifold?
8. Is disconnected positive circulation operationally acceptable in exported layouts, or should a
   later explicitly approved semantic/objective change discourage or prohibit it?
9. Are control ports, underground logistics, power, height, construction access, blueprint node
   conversion, or other source-table mechanics in scope for the spatial contract? They are not in
   the present model.

Until these questions are resolved, propagators may reason from the confirmed constraints but
must not turn an open ambiguity into domain deletion.
