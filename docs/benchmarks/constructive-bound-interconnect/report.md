# Recipe-bound fanin, fanout and planned crossing

## First milestone

Five actual catalog facilities and all local boundary obligations validate without changing the
production geometry rules. Three grinders feed two thickeners through one converger, two splitters
and one preplanned bridge. All twelve recipe terminal rates are covered, including five external
boundary connections. Used geometry is 35 x 22 with 129 physical belt cells (130 commodity-cell
visits, since two different networks share the bridge). This is a complete **local fixture**, not
the heavy-xiranite production graph or the normal production acceptance path.

A0/A1 run `grinder-plant-moss-powder-2-1` at 1/4 utilization each. Their combined output is 1/2 item/s.
B runs `grinder-plant-moss-powder-3-1` at 1/6 utilization and supplies 1/4 item/s. Each consumer runs
`thickener-plant-moss-enr-powder-2-1` at 1/4 utilization, taking 1/4 of powder 2 and 1/8 of powder 3
per second. The two commodities never merge. Raw inputs here mean inputs to this local fixture;
they are intermediate commodities supplied by the surrounding factory, not a claim that the recipe
book classifies them as globally external resources.

All recipe quantities, durations, facility dimensions, ports and component capacities load from
runtime catalogs. The external fixture file selects recipes, utilization and geometry. The existing
adjacent-swap module supplies the crossing geometry; the surrounding routes are explicit fixture
paths, not an N:M auto-placement solver. This deliberately exercises merge/split validity, not
minimal topology: the same-item pair allocations can be improved in the next milestone.

## Independent validation contract

`aic-witness-probe` is a new read-only diagnostic binary. It accepts a strict v1 witness envelope,
checks bounded arithmetic/count/coordinate inputs, derives recipe rates independently and invokes
the unchanged constructive network validator. The physical checks include port geometry, direction,
commodity normalization, conservation, capacity, component arms and supply/demand reachability.
Existing neutral geometry DTOs now support deserialization; no production route or solver changes.

Every expected facility/direction/item rate is compared with routed totals. Missing networks cannot
erase obligations: their rates remain in the ledger. The report distinguishes covered geometry
validity from complete recipe coverage and always keeps `factory_validated: false`. Arbitrary
preexisting pair-flow allocation is not imposed; compatible commodity margins are the contract.

The CLI writes detailed JSON only, with hashes of the witness and runtime catalogs. Exit status zero
means covered geometry valid, two means a rejected witness with a report, and one means CLI/input
failure. A successful partial witness still reports incomplete recipe coverage. This is neither a
global feasibility proof nor automatic production fallback.

## Negative controls and next capacity cliff

Eight release CLI controls are stored in `audit.json`. Removing the bridge, splitters or converger
is rejected even after deleting their network references. Wrong port and unknown item are rejected.
Omitting an entire external commodity network leaves valid remaining geometry but exposes pending
recipe rates. Multiplying all utilizations and rates by four stays within the selected recipe
utilization limits, but fails physical single-lane capacity. The actual overload picture highlights
the retained shared trunks and terminal approaches; no recipe infeasibility is claimed.

At full consumer utilization, each consumer needs two powder-2 lanes and one powder-3 lane. Each
A producer can independently supply exactly one consumer; preserving the original shared trunk is
unnecessary. The next slice will assign exclusive A producer/consumer pairs, use two output/input
ports per pair and bind B to one lane per consumer. Reassignment should precede geometry expansion.

## Verification and timing

- Five new Rust tests pass, including malformed rates/coordinates, identity errors, neutral DTO
  roundtrip, missing obligations and full-rate capacity rejection.
- `cargo fmt --all`, `cargo check --workspace` and `cargo test --workspace` pass: 524 tests and two
  existing ignores. Eleven Node tests pass across the fixture and permutation diagnostics.
- All eight release CLI controls pass. Actual normal, overload and narrow-browser geometry is
  inspected; six viewport/theme checks pass. Colors and facility labels derive from witness data.
- Final optimized rebuild took 2m30s. Builds/tests/browser capture are not validation timings.
  Initial fixture generation took 9.13 ms; initial optimized geometry/recipe validation took
  0.636 ms. These are small single observations, not production performance claims. Final control
  execution times, including separate process wall time, remain in `audit.json`.
- Reviewed against AGENTS and the constructive planner design: isolated diagnostics, unchanged
  physical validation and connected production construction. No game catalog or exact solver edit.

```sh
node docs/benchmarks/constructive-bound-interconnect/generate.mjs \
  --recipes data/game/normalized/recipes.json --facilities data/game/normalized/facilities.json \
  --items data/game/normalized/items.json --transports data/game/normalized/transports.json \
  --components data/game/normalized/logistics-components.json \
  --fixture docs/benchmarks/constructive-bound-interconnect/fixture.json \
  --case merge-split-cross --external connected --witness-output NEW_WITNESS --detail-output NEW_DETAIL
target/release/aic-witness-probe --witness NEW_WITNESS \
  --recipes data/game/normalized/recipes.json --facilities data/game/normalized/facilities.json \
  --items data/game/normalized/items.json --transports data/game/normalized/transports.json \
  --components data/game/normalized/logistics-components.json --report-output NEW_VALIDATION
```

`audit.mjs` accepts the same catalog paths plus `--validator`, `--witness`, a new `--work-directory`
and `--report-output`. `render.mjs` accepts `--detail`, `--validation`, `--transports` and a new
`--output`. The tailnet page is `http://100.114.24.86:8765/bound-interconnect.html`.
