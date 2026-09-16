# Recipe routing negotiation prototype

## Question

Does bounded negotiated congestion improve routing on the exact same frozen
recipe obligations and sequence-pair placements as the irreversible sequential
baseline, while allowing every compatible directional facility port and explicit
external boundary terminal?

## Prototype boundary

The research path in `aic-routing` accepts immutable jobs carrying the original
wiring ID, item, transport kind and exact rate. Each job has a domain of source
and target terminals. Facility domains contain every compatible projected port;
external input, target and surplus endpoints use directional cells on the fixed
canvas boundary. Facility bodies are hard obstacles on both transport layers.

The sequential baseline processes the stable wiring order once. A selected path
and terminal pair become exclusive immediately and are never reconsidered. The
negotiated variant routes every job while allowing temporary same-layer cell
overuse. It charges current occupancy and accumulated historical congestion,
then rips up all guides, prioritizes conflicted jobs and reselects paths and ports
for at most 24 iterations. Both modes use the same bounded multi-source,
multi-target grid search and input domains.

The result is a global-guide comparison. `legal` means zero unit-capacity guide
overflow, not a validated factory. The prototype does not yet merge compatible
same-item flows, check shared transport capacity, select splitters or convergers,
or legalize orthogonal crossings with catalog bridges. Reports mark both
shared-network and exact-component legalization as false.

## Controlled panel

The bounded panel used two real target families (`item-proc-battery-5` and
`item-xiranite-enr-powder`), nominal counts 8, 16 and 32, all three placement
profiles, and seeds 0 and 1. This scheduled 36 rows. One compact row retained the
placement-only `packing-target-missed` failure. Twelve other compact rows placed
nonoverlapping bodies but had at least one empty compatible terminal domain
because a neighboring facility body covered every matching port escape cell. No
compact row reached the routing comparison.

The remaining 23 rows compared both routers:

| Profile | Compared rows | Obligations | Sequential routed | Negotiated routed | Sequential zero-overflow | Negotiated zero-overflow | Negotiated routed all |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Balanced | 11 | 473 | 293 (61.9%) | 394 (83.3%) | 1 | 1 | 6 |
| Spacious | 12 | 549 | 433 (78.9%) | 549 (100%) | 1 | 1 | 12 |

Negotiation materially improved guide coverage and removed every unresolved job
in every spacious row. It did not improve the count of zero-overflow guide sets.
The best complete negotiated guides still had overlaps that the unit-capacity
model could not eliminate.

Two 96-iteration sensitivity runs separated the fixed 24-iteration bound from a
simple budget explanation. The 8-facility balanced battery case retained two
overflow cells, selecting the same best result at iteration 3. The 16-facility
spacious xiranite case improved from three overflow units at 24 iterations to two
at iteration 47, but did not reach zero. Runtime stayed bounded in these small
cases; extra iterations did not close the modeling gap.

Inspection of the battery sensitivity result found both missing semantics at the
remaining overlaps. One overlap carried two flows of the same item and is a
candidate for a capacity-sufficient shared trunk. The other joined different
items and is a candidate for an orthogonal bridge if exact directions and catalog
capacity permit it. Treating both as permanently exclusive cells makes zero
overflow too strict for the game rules.

## Decision

Do not integrate this guide router as a layout admission authority. Retain it as
the deterministic comparison kernel and preserve its useful properties:

- identical frozen obligations and poses for both modes;
- flexible port and external-terminal domains;
- completion-first lexicographic scoring;
- bounded work, compact conflict metrics and deterministic rerouting; and
- recoverable job identity, selected terminals, paths and overflow cells.

The next routing slice must group compatible equal-item flows into logical
networks with capacity accounting and classify or legalize catalog-backed
crossings before evaluating zero-overflow yield. Placement must also reject or
repair empty port-escape domains before routing. Only independently validated
shared networks and components can authorize complete-layout admission or a
revision of the packing-density bands.

## Reproduction

Build the release probe and run a frozen comparison with:

```sh
cargo build --release -p aic-cli --bin aic-recipe-leaf-probe
target/release/aic-recipe-leaf-probe \
  --recipes data/game/normalized/recipes.json \
  --target-item item-proc-battery-5 \
  --facilities 8 --seed 0 --attempts 32 --max-steps 64 --max-depth 4 \
  --catalog-dir data/game/normalized \
  --placement-density-min 0.13 --placement-density-max 0.30 \
  --compare-routing --routing-iterations 24 --routing-work-per-route 100000 \
  --report-output target/recipe-routing.json
```

The report schema is version 5. It records the semantic-to-block placement,
every routing job and candidate-domain size, sequential and negotiated metrics,
selected terminals, guide cells and exact overflow coordinates. Panel outputs
are regenerable and remain under ignored `target/`.
