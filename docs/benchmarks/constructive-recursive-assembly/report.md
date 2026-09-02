# Recursive Constructive Assembly

## Purpose

This slice tests repeated hierarchical composition rather than one module-to-facility connection. An external assembly request names a target facility and an ordered list of process modules. The assembly engine performs the same operation for every entry:

```text
construct one valid process module
-> treat the current partial factory as one immutable target node
-> move and rotate the new module as one immutable source node
-> select one compatible boundary-port pair
-> route the named logical requirement
-> validate every occupied cell and remaining boundary
-> return the result as the next constructive node
```

No step contains Heavy-Xiranite-specific behavior. The representative request contains the two Xiranite-powder producer modules required by the final oven.

## Result

- Status: constructed
- Release wall time: 2.03 seconds
- Requested modules: 2
- Completed modules: 2
- Final facilities: 7
- Final routed internal requirements: 6
- Final bounds: `22x9` (area 198)
- Final occupied belt tiles: 25
- Final occupied pipe tiles: 0
- Final route turns: 0
- Remaining boundary requirements: 8
- Remaining boundary port options: 18

| Step | Added block | Facilities | Bounds | Transport tiles | Turns | Blocked boundary options |
| ---: | --- | ---: | --- | ---: | ---: | ---: |
| 1 | powder module 0 | 4 | `14x8` | 9 | 0 | 1 |
| 2 | powder module 1 | 7 | `22x9` | 25 | 0 | 0 |

At step two, the four-facility result from step one is used directly as the target `ConstructiveNode`. Its placements and three routes are not rebuilt. The new three-facility module is rigidly transformed and joined through the second Xiranite-powder requirement. The final page visually highlights only the three facilities introduced by that module.

## Search Evidence

| Metric | Step 1 | Step 2 |
| --- | ---: | ---: |
| Whole-node rotations considered | 4 | 4 |
| Whole-node placements considered | 776 | 1,532 |
| Colliding placements rejected | 238 | 703 |
| Boundary port pairs considered | 13,450 | 16,580 |
| Blocked port pairs rejected | 496 | 503 |
| A* searches | 12,954 | 16,077 |
| A* failures | 0 | 0 |
| Future-boundary dead ends rejected | 950 | 399 |
| Valid candidates scored | 12,004 | 15,678 |

The second step is larger but remains a small local search over two nodes. The search never enumerates new positions or ports for the four facilities already accepted in the target composite.

## Remaining Interfaces

The final composite preserves:

- four enriched-carbon-powder belt inputs for the two powder modules;
- two clean-water pipe inputs;
- one liquid-Xiranite-poly pipe input for the final oven;
- one enriched-Xiranite-powder belt output.

Every boundary retains at least one physical port option. They are interface domains, not occupied transport cells.

## Interpretation

This result proves that the constructive node contract is recursively closed for repeated module attachment: a facility can become a composite, and that composite can become the target of the same operation again.

It does not yet prove complete Heavy Xiranite construction. The current assembly request manually identifies module roots, internal items, requirements, and order. Shared upstream subgraphs, cycles, mixed belt/pipe module families, automatic decomposition, and recovery from an exhausted module attachment remain future work. Those failures can now be isolated to one composition boundary instead of replaying the complete 59-facility search.

## Artifacts

- `report.json`: machine-readable two-step history and final composite node.
- `heavy-xiranite-two-powder-modules.html`: localized two-page assembly history.
- `data/examples/constructive-assembly.game-heavy-xiranite-powder.request.json`: explicit representative assembly request.
