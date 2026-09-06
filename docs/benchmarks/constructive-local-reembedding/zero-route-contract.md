# Explicit rerouting of recognized zero-segment connections

Chain schema 2 already recognizes isolated equal-rate facility source/sink pairs
sharing one transport cell. Explicit rip-up must be able to restore such a chain
without inventing a segment or rejecting a valid retained endpoint coincidence.

The path API gains an explicit boolean `allowZeroLength` (default false). After
ordinary input, bounds and blocked-endpoint checks, coincident endpoints return a
one-cell, zero-step, zero-turn path only when this flag is true and neither endpoint
requires a directed first/last segment. Other coincident endpoints retain the
`same-endpoint` diagnostic. No component arm is satisfied by an empty route.

The reembedding engine enables this only for an extracted schema-2 `zero_length`
chain, whose source/sink identities and rates were checked by `detachablePairs`.
It does not authorize arbitrary nonzero chains to collapse or relax collision,
commodity, capacity, port, or original-graph validation. Separated endpoints still
use ordinary A*. Existing reconstruction keeps the single occupied cell and no
segments. All promoted factory candidates continue to validate independently.

Regression evidence: opt-in/out, malformed flag, blocked/outside endpoints, directed
arms, immutable input, explicit recognized-chain reroute and actual-factory failed
request replay. A successful path or private replay is not complete-factory success.
Compare frozen pre-fix reports; do not attribute remaining route-order or placement
failures to the equal-endpoint bug after it has been removed.
