# Coincident terminal detachment and remaining packing limits

## Outcome

The complete incumbent improves from 47x57 / 438 transport cells / 38 turns to
46x54 / 425 / 40. Used area falls from 2,679 to 2,484 (7.28%). All 59 facilities,
eight logistics components, original requirements and external margins validate.
This is an improvement, not a resolution of the large interior-space problem.

The contract is in `terminal-identity-contract.md`; the selected artifact and audit
references are in `terminal-identity-results.json`. Large trial records remain in
JSON files under `target/`, not repository content or terminal output.

## Representation cutover

The old coordinate-keyed ownership made a coincident F36 output and F25 input an
inseparable movement group. This was an implementation restriction, not a game rule.
Chain schema 2 represents an isolated, uniquely identified, equal-rate source/sink
pair as an explicit zero-segment chain. Translating either owner can now turn it
into an ordinary routed connection in one private final-state transaction.

Movement, group screening, route endpoints, port assignment and diagnostic replay
now use terminal identity for this case. Mandatory coordinate closure is removed
only for supported isolated pairs. Complex junctions remain coupled. Old chain-ID
replay reports are rejected explicitly instead of silently replayed incorrectly.
No witness schema, production API, catalog or Rust solver behavior changes.

Singleton proposals also allow increased endpoint distance and rank body envelope
before distance. Incident bridge release is available to facility-movement families.
These are proposal heuristics, not admissible bounds on routed area. Every promoted
candidate must improve actual routed area/cell/turn score and validate independently.

## Experiments

All trials use runtime catalogs, independent belt/pipe compaction, flexible terminal
directions, downstream junction ownership, incident bridge release, and at most eight
new crossings. Singleton batches use three rounds and up to 512 positions per round,
with near/far route order controls. Times are optimization-batch elapsed times, not
initial factory generation latency or controlled performance benchmarks.

| Experiment | Repair attempts | Area / cells / turns | Seconds |
| --- | ---: | --- | ---: |
| Initial singleton exploration | 2,784 | 2,530 / 427 / 43 | 136.66 |
| Frozen-source initial repeat | 2,784 | 2,530 / 427 / 43 | 140.23 |
| Singleton continuation | 2,784 | **2,484 / 425 / 40** | 135.20 |
| Initial coalescence portfolio | 776 | 2,632 / 443 / 39 | 24.28 |
| F36 alternative continuation | 2,784 | 2,484 / 432 / 44 | 153.20 |
| Coalescence from selected incumbent | 178 | unchanged | 5.91 |

The initial exploration preceded the final global terminal-ID uniqueness check.
The frozen-source repeat reproduces the selected witness byte-for-byte. Repeat,
continuation and F36-branch report source hashes match the committed engine sources.
Within a round, records marked selected are successive best siblings from a fixed
parent, not cumulative ancestry. In particular, the F36 upward move is an alternative,
not an ancestor of the selected 2,484 / 425 / 40 witness.

The final coalescence run starts 22 of 750 available pair/link combinations before
its 250,000-pose bound; 89 positions yield 178 repairs, 168 rejected and ten unimproved.
This bounded result is not exhaustive and does not prove local or global infeasibility.

## Visual findings and next decision

Personally inspected the native full and close-up selected render and the full F36
ABA alternative. The upper process banks still retain substantial vertical separation.
The selected lower layout leaves F36 away from F37/F48, with a long seed return.
The alternative joins F37/F48/F36 into ABA, but transfers a long perimeter detour to
the F36-to-F25 output. Its area ties the selected result and its wiring is worse, so
it is not promoted. Gathering a cycle alone is not enough: consumers and interfaces
must participate in coordinated repositioning.

The present search preserves rigid group shapes and much of the routing topology.
The axis compactor preserves perpendicular geometry and relative order. Improvement
acceptance keeps one best factory; private proposals explore a bounded neighborhood,
not arbitrary coordinated rearrangements. These restrictions can explain why visible
space survives, but do not prove that a particular hand-drawn shift will route.

Next evaluate a bounded placement-oracle experiment around a complete process bank
and its consumers. Pumpkin is a promising candidate, not an adopted replacement.
Define movable facilities, optional group shapes, port-access constraints, fixed
boundary interfaces, budgets and route-failure diagnostics before implementation.
Compare identical neighborhoods against the heuristic proposals. Release and rebuild
affected interfaces, score actual routed bounds, and keep the complete incumbent on
failure. A placement-only objective is a proxy; an unroutable packing is not success.
Do not reintroduce the former global joint placement-routing solver into production.

## Verification and scope

- 212 Node tests pass; 524 Rust tests pass, with two existing ignores.
- `cargo check --workspace` and `cargo test --workspace` pass; no Rust edits.
- Eight original-graph controls and nineteen crossing controls pass.
- Selected and ABA-alternative native pages pass six viewport/theme checks each.
- Unit tests cover separate/equal shifts, identity-specific ports, exact rates,
  immutable input, no ghost cells, ambiguous ownership and body collision.
- Synthetic geometry tests are not substitutes for full actual-catalog validation.
- Movement screening still precedes port reassignment; rejection is not a proof
  that another joint port/movement choice is impossible.

Compared against AGENTS.md and the constructive-planner design: all work remains
isolated complete-factory improvement research, promoted states validate, used area
remains first, and the production constructor and solver boundary are unchanged.
The broader research/product goal remains open; no premature Rust port or global
optimality, infeasibility, or arbitrary-recipe claim is made.
