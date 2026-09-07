# Production-learning baseline and visual audit

Status: stage 1 measured; goal active; not production-ready; impossibility not established.

## Reference geometry

All four complete 59-facility witnesses passed fresh Rust validation and the original
Heavy Xiranite production/boundary coverage audit. Common current scoring counts directed
turns including facility arms; historical turn counts are not directly comparable.

| Reference | Used area | Transport tiles | Turns | Empty XY fraction |
| --- | ---: | ---: | ---: | ---: |
| Original constructor | 12,900 | 2,401 | 268 | 73.2481% |
| Refined constructor | 4,032 | 879 | 193 | 51.9841% |
| Previously selected learned example | 9,548 | 1,769 | 236 | 70.5279% |
| Strongest recorded geometric incumbent | 2,132 | 392 | 147 | 29.6904% |

The learned example is a selected observation, not a typical-result estimate, and is not
competitive with the strongest historical geometric search. That historical search used a
different search budget; this table is a capability reference, not a matched-budget comparison.

The strongest geometry occupies 41 x 52 cells: 1,133 facility cells, 274 belt cells and 118 pipe
cells, with 26 overlapping belt/pipe XY positions. Component cells are already included in
transport occupancy. The occupied union is 1,499 cells; 633 are unused. The largest entirely
empty rectangle is `(27,26)` with width 7 and height 10. No known reference passes the 1% gate.

Witness file SHA-256 after canonical JSON export:
`38829b5ec2949cfa3a04542a5faf73979b2802c011cebde69733e4bcae104299`.
The original selected-file hash differs because serialization differs; source verification
occurred before export. The exact JSON-value digest is
`989c44e2ca51dcf7d6712faeb8a11efed76509cdb37d3024b70a4dc5913c5a07`.

## Direct visual inspection

Inspected the actual exported geometry at 736 px in light mode and 360 px in dark mode.
Automated browser checks additionally covered both widths in both themes, all 59 facility
marks, transport marks, readable displayed labels, no horizontal overflow and working
facility/network selection. Hidden small labels retain clickable geometry and detailed text.

Observed: a large middle-right void separates process groups; additional clear strips and
smaller gaps remain between left-side production groups. Right-side pipe runs are mostly
straight with explicit external terminals; lower and left-side belt return paths connect
separated facilities. The density requirement visibly fails. No visual claim of gratuitous
loops or routing optimality is made.

Across fixed extracted chain endpoints, 317 segments exceed their obstacle-free Manhattan
distance by only 2 segments, both in chain 39 of `network:item-plant-moss-seed-3`. Thus merely
shortening existing fixed-endpoint chains has little length headroom in this incumbent.
This is not a lower bound for changed placements, ports, junctions or topology. The next
useful experiment should expose relative group motion, rotation and endpoint reassignment,
not assume fixed-endpoint rerouting can remove the large packing gaps.

Screenshots and interactive source are under `target/layout-production-visual-01/`;
`source.json` binds the geometry and input report to hashes, and `browser-check.json` binds
the fragment to four browser checks. No image generator or hand-adjusted geometry was used.

## Existing policy waste

Freshly validated 108 retained learned-policy outputs from the existing one-seed validation
pilot: four models, nine factories, and proposal caps 128, 512 and 1,024. No sealed test
evaluation was performed. At 1,024 proposals, mean per-factory mechanical waste is
88.1619–88.7587%, with a 99.6094% 95th percentile for each model. At 128 proposals,
means are 82.7257–83.3333%. These fail the working efficiency thresholds.

Waste includes rejection, unchanged output and exact-state revisits; valid temporary
deterioration is reported separately, not classified as waste. This audit does not yet
attribute rejection to static impossibility, routing resource exhaustion or poor policy
selection. That decomposition belongs to the next controlled action-capability probe.

## Reproduction and verification

Run with a new output directory:

```sh
node docs/benchmarks/layout-production/baseline.mjs --plan docs/benchmarks/layout-production/baseline.json --output-directory target/layout-production-baseline-03
node docs/benchmarks/layout-production/visual.mjs --report target/layout-production-baseline-03/report.json --output-directory target/layout-production-visual-01
node --test docs/benchmarks/layout-production/metrics.test.mjs
cargo check --workspace
cargo test --workspace
```

All commands passed. Five metric tests include exhaustive comparison of the largest-empty-
rectangle algorithm on every 3 x 3 occupancy mask, layered occupancy, exact 1% boundaries,
waste categories, coordinate overflow and early geometry-budget rejection. Reusing an output
directory was manually checked: it returns a structured `ready-baseline-failed` path error
before spawning the validator and does not overwrite results. Browser verification uses the
explicit local Chromium and theme CSS paths through `browser-check.mjs`.

The baseline and visual CLI consume already generated historical artifacts; those artifacts
must exist and match their recorded hashes. This is an exploration harness, not production
inference. No Rust/production planner code, game data, solver semantics or dependencies changed.
Compared against the accepted constructive planner design: full valid factories remain the
input/output invariant, no facility-only cutover or exact-solver fallback was introduced,
and measured dimensions are outputs, never production feasibility bounds.

Next: define and measure a finite action-domain experiment, with charged preflight work,
single-axis ablations and interactions, before changing training architecture or integration.
