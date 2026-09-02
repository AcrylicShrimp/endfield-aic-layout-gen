# Constructive Powder Process Module

## Purpose

This slice tests hierarchical constructive planning at the exact cluster that blocked the second Heavy Xiranite belt frontier. Instead of adding enriched-carbon suppliers after ten facilities and nine routes are already frozen, it constructs the local process as an independent module:

- one Xiranite-powder oven as the module root;
- two enriched-carbon furnaces supplying that root;
- both enriched-carbon belt requirements routed inside the module;
- every graph edge crossing the module boundary exposed as a logical requirement with physically available port options.

The CLI receives a root facility instance and one facility-supplied internal item. It contains no Heavy-Xiranite-specific IDs in application code.

## Result

- Status: constructed
- Release wall time: 0.62 seconds
- Facilities: 3
- Internal routed requirements: 2
- Used bounds: `9x7` (area 63)
- Internal belt tiles: 4
- Internal route turns: 0
- Boundary requirements: 4

| Boundary | Direction | Transport | Rate | Exposed port options |
| --- | --- | --- | ---: | ---: |
| enriched-carbon powder for furnace 0 | input | belt | 1/2 s | 3 |
| enriched-carbon powder for furnace 1 | input | belt | 1/2 s | 2 |
| clean water for the Xiranite-powder oven | input | pipe | 1/2 s | 1 |
| Xiranite powder from the oven | output | belt | 1/2 s | 5 |

The boundary connections are not physically routed. They are module interface domains: each option identifies a facility-owned port, its world position, its outward connection cell, direction, transport layer, item, and rate. The HTML shows every option as an input or output arrow without counting it as an occupied transport tile.

## Search Evidence

- Placement candidates considered: 8,820
- Overlapping placements rejected: 1,040
- Port pairs considered: 4,161
- Blocked port pairs rejected: 1,224
- Future-port dead ends rejected: 6
- A* searches: 222
- A* failures: 0
- Valid candidates scored: 196
- Placement area-bound prunes: 7,486
- Endpoint area-bound prunes: 2,715

The crowded ten-facility prefix previously spent 130,552 A* searches on the next enriched-carbon connection and still exhausted. The isolated three-facility module solves the complete two-connection local process with 222 A* searches. These are not equivalent optimization problems, but the contrast confirms that the hard part was the frozen global context rather than this production process in isolation.

## Scope And Remaining Work

This result proves the first module boundary and construction contract. It does not yet prove that `9x7` is the module's mathematical minimum, automatically decompose the full production graph, choose one fixed boundary-port assignment, or deploy multiple modules as macros.

The next experiment should construct the existing three-facility liquid process as a second module, place both immutable module geometries as macro candidates, and route the Xiranite-powder connection between their exposed interfaces. A failed macro connection must leave both original module artifacts intact and report the rejected interface pair.

## Artifacts

- `report.json`: machine-readable module members, internal routes, boundary interface domains, and search statistics.
- `heavy-xiranite-powder.html`: two-page construction history with internal belts and clickable boundary port arrows.
