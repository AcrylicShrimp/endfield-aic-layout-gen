# Heavy Xiranite Factored-Network Decomposition

## Scope

This checkpoint applies the exact `joint-shared-transport-layer-factored-endpoints-v1`
formulation to cleanly rebuilt network subsets of the 12 by 12 cumulative SCC phase-zero input.
Every case received an independent five-second release-mode search budget. No placement, port, or
route decision was fixed, and no constraint family was removed.

## Results

| Case | Networks | Logical requirements | Terminals | Variables | Constraints | Terms | Factor incidences | Placement-routing incidences | Build ms | Search ms | First incumbent ms | Result |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| `single-0` | Xiranite ENR Powder | 1 | 2 | 8,821 | 31,383 | 111,020 | 110,684 | 30,512 | 70 | 2,806 | 1,069 | optimal, validated |
| `single-1` | Xiranite Powder | 2 | 4 | 9,104 | 32,781 | 117,741 | 117,125 | 36,788 | 54 | 5,015 | 4,274 | unknown, invalid witness |
| `single-2` | Liquid Xiranite Polymer | 1 | 2 | 8,709 | 30,807 | 108,316 | 108,092 | 26,544 | 48 | 2,897 | 1,925 | optimal, validated |
| `pair-0-1` | two belt networks | 3 | 6 | 9,443 | 37,007 | 135,602 | 134,650 | 52,836 | 70 | 5,002 | none | unknown |
| `pair-0-2` | belt and pipe | 2 | 4 | 17,120 | 55,465 | 192,705 | 192,145 | 50,512 | 99 | 5,000 | none | unknown |
| `pair-1-2` | belt and pipe | 3 | 6 | 17,403 | 56,863 | 199,426 | 198,586 | 56,788 | 103 | 5,000 | none | unknown |
| `full` | two belt and one pipe | 4 | 8 | 17,742 | 61,089 | 217,287 | 216,111 | 72,836 | 110 | 5,000 | none | unknown |

The seven-case batch used 183,582,720 bytes of maximum resident memory as reported by macOS
`/usr/bin/time -l`. This is a batch peak, not a per-case RSS comparison.

## First Boundary

Network count alone is not the first boundary. Both one-requirement networks solve to validated
optimality, while the two-requirement Xiranite Powder network reaches its first solver incumbent at
4,274 ms but does not finish the primary area stage. Its retained incumbent contains active
transport geometry that the post-solve witness validator rejects because it is not on a directed
supply-to-demand flow path.

This exposes two separate effects:

1. combining two logical requirements into one same-item network increases endpoint state and
   placement-routing coupling enough to consume almost the entire budget despite only a 3.2 percent
   total-variable increase over `single-0`; and
2. when the primary area objective times out, transport-tile minimization has not yet removed
   solver-legal zero-contribution geometry, while validation currently rejects that geometry.

The two-network cases are a later boundary. `pair-0-1` reuses one belt grid and adds only 339 total
variables over `single-1`, but placement-routing incidences rise from 36,788 to 52,836 and no
incumbent appears. Mixed belt-pipe pairs instead instantiate both physical layers and nearly double
the total variable count.

## Next Decomposition

The next exact checkpoint must split the hard Xiranite Powder network by its two logical
requirements and rebuild each one independently. That determines whether either requirement is
intrinsically hard or whether the first cliff is specifically their shared same-item network. If
both singles are tractable, the following diagnostic must separate endpoint/terminal coupling from
shared-network flow and topology constraints without treating any relaxed result as a production
layout.

## Artifacts

The complete matrix and per-case JSON/HTML artifacts are under
`docs/benchmarks/heavy-xiranite-factored-network-decomposition/`.
