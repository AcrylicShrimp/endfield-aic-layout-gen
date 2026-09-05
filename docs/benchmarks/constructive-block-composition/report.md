# Actual graph block composition

Date: 2026-09-06. Status: validated partial research milestone, not a complete factory.

## Result and scope

The retained eight cyclic blocks now feed the actual processing frontiers. No supplier is duplicated
and no unfinished graph obligation is replaced with an external source. This is a compound-block
experiment; production construction and its connected transactional contract remain unchanged.
The shared geometry-only port planner replaces duplicated interface planning without introducing
an adapter to production or a Rust migration.

| Case | Facilities | Used bounds | Area | Physical tiles | Turns |
| --- | ---: | --- | ---: | ---: | ---: |
| Partial shared supplier | 9 | 30 x 29 | 870 | 211 | 38 |
| Both shared-supplier consumers | 14 | 31 x 43 | 1,333 | 336 | 56 |
| First full belt frontier | 41 | 79 x 128 | 10,112 | 1,932 | 248 |
| Second full belt frontier | 47 | 83 x 128 | 10,624 | 1,938 | 248 |
| Third full belt frontier | 53 | 87 x 128 | 11,136 | 1,944 | 248 |

The packed-row baseline ended at 99 x 129, area 12,771, with 3,450 physical tiles and 272 turns.
Preserving source heights before compaction reduces area by 12.8% and physical tiles by 43.7%.
These are the same 53 original facility identities and recipe utilizations, not different production
targets. Component count is diagnostic only: a smaller valid footprint wins even if it needs more
splitters, convergers or bridges. No global or local minimum-area proof is claimed.

The nine-facility case leaves F42's other 0.5/s powder output pending. In the fourteen-facility case,
that remainder is consumed exactly once. Same-commodity pairwise graph allocations may redistribute
within the selected original source/consumer margins; literal original edge-to-edge pairing is not
claimed. Every retained interior segment, rate and port terminal is preserved under whole translation.
The last twelve facility additions form straight process chains: each connection occupies exactly
one shared source/target connection cell and no edge between distinct cells.

There are still six unplaced mixed-transport facilities (F00, F01, F02, F03, F04 and F09), six pending
carbon output recipe terminals and seven original external graph obligations. The current partial
witness may contain multiple connected regions. Requesting a fourth belt-only frontier returns
`composition-no-ready-belt-frontier`, exit 2, and the last accepted 53-facility witness. This is an
explicit scope boundary, not proof of infeasibility or a completed heavy-xiranite graph.

## Geometry and measurements

Order whole source regions and ready consumers with a bounded insertion search. Choose free output
ports with clear eastward rays. Place consumers near their source port rows and assign capacity-safe
commodity buckets. The common `planPortInterface` builds the exact fixed split/merge/cross geometry.
It handles either packed rows or nonuniform source-preserving rows; the latter transforms complete
paths and junctions together. Independently validate both raw families and both compacted variants,
then select by actual area, transport cells and turns. Port pressure/inversions/fragment count guide
the internal ordering heuristic, not the final geometric acceptance score.

Final quiet CLI run, Node v26.5.0 and the existing optimized Rust witness probe:

| Frontier | Generate both families (ms) | Compact both (ms) | Order search (ms) | Stack/index (ms) | Node setup (ms) | Allocation (ms) |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 41 facilities | 461.85 | 67.49 | 400.06 | 205.02 | 160.60 | 29.66 |
| 47 facilities | 74.44 | 31.47 | 36.74 | 5.92 | 26.35 | 2.17 |
| 53 facilities | 67.90 | 29.62 | 31.81 | 5.47 | 22.89 | 2.00 |

Generation totals 604.19 ms. Full three-stage CLI elapsed time is 1,355.20 ms, including four parallel
candidate validators per stage and JSON/HTML output, excluding initial catalog/source-block loading
and prior cyclic-block generation. These are single-run observations, not stable benchmark medians
or whole-factory timings. Portfolio cloning is included in generation. Order-search columns overlap:
stack, setup and allocation are subparts, not additional costs to sum. Full per-candidate geometry,
lowering and validator timings are in `matrix.json`.

First-stage ordering uses all 128 trials, with 87 stack builds and 41 cache hits. Each later stage has
31 trials, one build and 30 hits. A bounded eight-entry cache preserves exact geometry; a rightmost
occupied-cell row index makes eastward exit checks independent of ray length. Future runtime work
can index graph/catalog lookups in node setup. Geometry synthesis itself is not the dominant cost.

## Visual inspection

Rendered and personally inspected the full 53-facility witness, the fourteen-facility shared-supplier
case, a bridge close-up and F16 -> F10 -> F05. Source-height preservation removes large gathering
detours. The direct chain is genuinely one cell apart; the bridge has orthogonal straight channels
with a separate nearby bend, not an illegal same-cell turn. The full view still has an obvious wide
inter-bank void: serial crossing columns, reserved approach bends and unequal stacked-block widths
keep consumers farther away than necessary. This is the next space-quality target after mixed-layer
head closure, not evidence that current dimensions are optimal.

The common diagnostic renderer now distinguishes additional internal commodities by color and adds
wheel zoom, pointer pan and Fit. The final 53- and 14-facility pages each passed six theme/viewport
checks (light/dark, widths 1400/736/360), wheel zoom, actual pointer-drag pan and reset checks. Actual
browser captures are under `target/shared-results/bound-interconnect-composition-{belts,fourteen}.png`;
the belt figure also has `-chain` and `-crossing` close-ups. Tailnet pages use the same basenames with
`.html` at the existing port 8765 server. No new hosting service is introduced.

## Verification and independent review

- All 20 emitted candidates across the nine-, fourteen- and three-stage cases passed the independent
  Rust geometry gate and original graph-margin coverage gate. No selected fallback masks a failure.
- Six controls passed: unchanged composition, missing shared-supplier remainder, broken retained
  interior segment, missing frontier commodity, duplicate retained owner and changed frontier
  utilization. Missing remainder/network cases deliberately pass geometry but fail graph coverage.
- 40 related Node tests passed, including existing interface/cycle regressions and new compound-block,
  pending-rate, one-cell-chain, ownership, transport, namespace and serialization tests.
- The existing automatic-interface audit passed all 64 expectations after the common helper change.
  `cargo check --workspace` passed; `cargo test --workspace` passed 524 tests with two existing ignores.
  No Rust source or dependency changed. Logs are `target/composition-{all-node-tests,cargo-check,cargo-tests}.log`.
- Read-only independent review found two reproducible defects. Recursively encoding prior IDs makes
  16 wrappers of a short ID reach 393,502 characters; arbitrary `__proto__` facility dictionary entries
  disappear after JSON serialization. Both were verified locally before changing code. Per-region
  numeric reindexing removes recursive escapes and null-prototype dictionaries are restored after
  portfolio cloning. Regression tests cover both fixes. Original facility/graph identities remain
  unchanged; generated physical requirement/component IDs are research-local.
  Focused independent re-review also confirmed both fixes, including a 41-to-47-facility composition
  with prototype-named IDs through portfolio generation, compaction and JSON serialization.
- The coverage helper was renamed to `inspectBlockCoverage` without an obsolete alias. Its historical
  `cycle-*` diagnostic codes remain stable; it still rejects all external terminals and requires
  closed inputs. Final boundary closure needs an explicit new contract, not silent relaxation.

Compared with the production design and repository policy: runtime catalogs remain authoritative;
no fixed canvas limit, global solver fallback, false infeasibility claim, production cutover or deferred
production routing was introduced. The multi-region research experiment is not claimed to satisfy
production's single connected-frontier or stable physical-history ID contract.

## Reproduce

Regenerate the selected cyclic-block baseline as documented in `../constructive-cycle-blocks/report.md`
if its witness directories are absent. Then run from the repository root with a fresh output path:

```sh
node docs/benchmarks/constructive-block-composition/run.mjs \
  --graph docs/benchmarks/constructive-chain-blocks/gap-1-placement.json \
  --blocks docs/benchmarks/constructive-cycle-blocks/matrix.json \
  --frontier ready --steps 4 \
  --recipes data/game/normalized/recipes.json \
  --facilities data/game/normalized/facilities.json \
  --items data/game/normalized/items.json \
  --transports data/game/normalized/transports.json \
  --components data/game/normalized/logistics-components.json \
  --validator target/release/aic-witness-probe \
  --output-directory target/composition-repeat
```

Exit 2 is expected after three accepted frontiers. Use `--steps 3` for the completed belt-slice exit 0.
For the shared-supplier gates use `--frontier F28 --steps 1` and `--frontier F28,F29 --steps 1` with
fresh output directories. `audit.mjs` takes `--baseline` pointing to the nine-facility `report.json`,
the same five catalog flags and validator, and a fresh `--output-directory` (no graph/frontier flags).
Frozen evidence: `nine.json`, `fourteen.json`, `matrix.json`, `controls.json`; detailed per-candidate
witnesses and diagnostics remain in the referenced target directories. Stdout is one short summary.
