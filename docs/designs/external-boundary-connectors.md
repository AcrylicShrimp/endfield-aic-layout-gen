# External Boundary Connector Contract

## Status

Accepted hard cutover. This contract replaces external terminals tied to a facility port or to the
fixed search-grid perimeter. There is no compatibility path for either old representation.

The three-template boundary connector rule is explicitly approved external-connection semantics.
It applies only to external inputs and final outputs. Internal facility-to-facility placement and
routing remain joint solver decisions.

## Meaning

An external connector is one of three deterministic belt or pipe stubs between a solver-selected
facility port and the final used-geometry bounding box. The game system connects the boundary exit
automatically beyond the blueprint.

- External input: the boundary exit supplies flow through the stub to an IN facility port.
- Final output: an OUT facility port supplies flow through the stub to the boundary exit.
- Facility-to-facility requirements remain ordinary internal routing.

Connector cells are real used geometry. They count toward width, height, area, transport-tile
count, turns, and collision. Nothing is routed beyond the boundary exit.

## Three Templates

For a selected rotated port, treat its outward cardinal direction as `0 degrees`. Construct exactly
three candidate connector templates:

1. `forward`: continue at `0 degrees` to the matching bounding-box side;
2. `left`: enter the port-adjacent connection tile, turn once by `-90 degrees`, and continue to the
   matching side; and
3. `right`: enter the port-adjacent connection tile, turn once by `+90 degrees`, and continue to the
   matching side.

The side behind the port at `180 degrees` is never a candidate. A left or right template turns in
the port-adjacent connection tile; the turn position is not a solver variable.

The solver always receives all three templates whose complete geometry fits the caller's hard
ceilings. It selects one template jointly with placement, rotation, port choice, and internal
routing. No distance-based preselection or tie rule is applied by the harness.

The lexicographic objective resolves the choice: bounding-box area first, unique physical transport
tiles second, then route turns and later tie-breakers.

## Derived Geometry

Every cell from the port-adjacent connection tile through the selected boundary cell belongs to the
connector. Forward has no turn. Left and right have exactly one turn. Connectors cannot contain a
splitter, converger, or bridge and cannot share a trunk with another connector.

If a candidate template collides with a facility, another external connector, or incompatible
internal transport geometry, that template is illegal. The solver may choose another template,
port, rotation, or placement.

The exit side is linked to final used geometry, never to `max_width`, `max_height`, or the fixed
search-domain perimeter. Caller bounds remain hard ceilings only.

## Solver Decisions

The joint solver chooses:

- facility placement and rotation;
- a compatible directional belt or pipe port;
- one of the three connector templates; and
- all internal facility-to-facility routing.

External connector cells are derived from those finite choices. They are not free path variables.

## Port and Flow Rules

A selected facility port must belong to the selected facility mode and rotation, match belt versus
pipe and IN versus OUT, carry the connector item and rate, and satisfy port occupancy and capacity
constraints with every other connector and internal route endpoint assigned to that port.

Logical wiring edges continue to express fungible material supply and demand for internal networks.
External requirements are the approved exception: each owns one connector template and cannot join
an internal or external shared trunk.

## Model Cutover

Model preparation separates external logical requirements from internal commodity networks. Each
external requirement creates an exact placement-port-template selector over legal Cartesian
combinations. Internal physical network normalization receives facility-to-facility requirements
only.

Derived connector geometry participates in transport collision, used geometry, bounding box,
transport-tile count, item, direction, rate, turn, and port-capacity constraints. It does not create
free grid arc, flow, branch, bridge, or arbitrary turn-position variables.

## Output DTO

Each successful connector witness records stable connector and requirement IDs, external node,
facility instance, port, item, transport kind, IN/OUT role, rate, selected template, ordered segment
cells, turn cell when present, boundary side, and exit cell.

The renderer draws every derived transport tile and the outward boundary arrow. It does not add a
tile outside the blueprint.

## Witness Validation

Validation proves that every external requirement has exactly one connector; the selected template
is one of forward, left, or right relative to the rotated port; no connector uses the rear side; its
cells exactly match the deterministic template through the reported used-bbox side; forward has no
turn and lateral templates have exactly one adjacent turn; all tile kind, item, direction, rate,
collision, port, and capacity rules hold; and no coordinate depends on unused search capacity.

Timeout remains `unknown`, never infeasible.

## Diagnostics

Stable diagnostics cover no compatible port, no legal connector template under hard ceilings,
template collision, invalid rear-side selection, port capacity overflow, malformed connector
witness, and any exit coupled to the search-domain perimeter instead of used geometry.

## Verification Baseline

Heavy Xiranite cumulative SCC phase zero has one facility and four external logical requirements.
It must jointly solve facility placement, rotation, compatible ports, and four three-way connector
choices, then derive their transport geometry. It must create zero free-routing commodity networks,
and no exit may be fixed to the 12 by 12 request perimeter.
