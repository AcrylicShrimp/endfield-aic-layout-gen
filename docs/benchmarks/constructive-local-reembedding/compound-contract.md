# Atomic multi-group relocation

Move schema 3 replaces the single `move` request with `moves`, an array of at most
four rigid translation primitives `{ instances, dx, dy }`. Each facility occurs once
across primitives; total selected membership remains at most sixteen. An empty array
means routing-only. Old `move` payloads are rejected, not automatically adapted. Historical
schema-2 reports must be replayed at their recorded commit or explicitly migrated.

Each primitive preserves the facility rotations and relative interior geometry. All
new bodies and terminal poses are evaluated simultaneously against the unchanged
incumbent, allowing exchanges that no sequential single move could realize. Shared
zero-length terminal owners must receive identical translations. A nonterminal junction
cluster can move only if its existing ownership policy yields one common translation.
Otherwise retain it and rebuild incident connections. External terminals never move.

An internal chain is carried only when both anchors receive identical translations.
Chains between differently moving groups and displaced unrelated chains are rerouted.
Reject new same-layer coincidences among carried geometry unless their original cell
identity was shared already. Bodies obstruct both layers. Preserve rates, port identities
unless explicitly reassigned, and the existing complete-factory validation gate.

The initial proposal family exchanges or repacks pairs of disjoint graph-derived groups
along either axis, with bounded body-envelope/locality ranking. It is a proposal family,
not a completeness or minimality proof. Compare against the same-incumbent single-group
control using identical routing, crossing, independent-layer compaction and work budgets.
Report geometry screening, routed attempts, failures, accepted whole scores and actual
native renders. Pumpkin remains an optional alternative, not a dependency of this slice.

Close proposal seeds transitively over shared same-layer facility terminal cells before
moving them. Include singleton seeds so small independent consumers are not excluded.
Drop a closed group only if it exceeds the explicit membership budget or shares a terminal
with a fixed external. This is closure under retained ports, not an immutable game grouping.
Bound pair enumeration (default 20,000) and report exhaustion separately from the position
shortlist. `--audit-unimproved 0..64` independently validates a bounded sample of fully
routed, non-improving movement candidates without promoting them; default zero preserves
the normal score-first validation cost. Reports distinguish unvalidated `unimproved`
candidates from `unimproved-validated` full factories.
