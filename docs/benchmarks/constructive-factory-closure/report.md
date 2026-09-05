# Complete heavy-xiranite graph closure

Date: 2026-09-06. Status: complete original graph validated; space-quality work remains.

## Result

The previous 53-facility belt composition now closes all six remaining mixed-transport facilities,
all original internal recipe margins and all seven original external requirements. The final witness
contains 59 unique original facilities, 131 fully covered recipe terminals, no pending recipe rate,
15 retained logistics components and a used 100 x 129 footprint (12,900 area). Physical transport
tiles total 2,401; route turns total 253. Component count is diagnostic, not an objective.

External inputs are water at an aggregate 2/s and sewage at 0.5/s. The original four water obligations
remain separately identified in the physical terminals (two share an original external endpoint ID).
The final target exports 0.1/s enriched xiranite powder east; the original low-polymer liquid surplus
exports 0.5/s south. No substitute external source or sink was invented to finish internal dependencies.

The independent Rust witness probe validates cell/port/layer geometry, capacity, flow and reachability.
The new independent graph-coverage check additionally matches every original member, recipe,
utilization, required terminal total and original external endpoint/item/direction margin. It does not
trust a candidate's claimed pending edges. Intermediate outputs remain exact graph cuts; complete
acceptance requires zero pending rate and the complete original membership. Original same-commodity
pairwise graph allocations may have been redistributed by earlier block synthesis while preserving
all per-instance margins; literal original edge pairing is not claimed for retained interiors.

This proves a complete steady-state witness under the loaded catalogs and existing validation model,
not minimum area, universally supported recipes, startup scheduling or the complete web-service goal.

## Construction and visual diagnosis

The source composition remains unchanged. Add each ready facility near retained supplier ports;
evaluate east-flow catalog placement and unused port-pair candidates together with finite orthogonal
corridor templates. Split obligations at transport capacity into independent facility ports. These
last obligations need no additional splitter/merge/cross component: the existing fifteen structured
components remain intact, and same-layer paths do not intersect. Pipe and belt layers may overlap
where the catalog validation permits it. This does not replace the earlier general junction research
with a claim that direct connections always suffice.

Accepted expansion order was F09, F04, F01, F02, F03, F00. All 48 facility candidates passed geometry
and partial graph coverage; all six selected states have complete inputs before further expansion.
All original retained segments, terminal ports and rates stay fixed through these growth steps.
The two final raw/strip-compacted output candidates both passed full validation. Total: 50 candidate
successes plus the independently rechecked baseline.

The first 59-facility run stopped only at surplus export: restricting every external terminal to north
made the unused F01 south port unable to return through the occupied pipe approaches. A rendered
close-up showed the obstruction. Allowing an actual south export closed that obligation without moving
facilities or removing prior routes. An initial two-cell export reservation made the product choose
a north detour because using the east edge cost an extra footprint column. Reducing the generated
reservation to one cell lets the target leave immediately east. Area stays 12,900; transport drops
from 2,455 to 2,401 tiles. The selected raw candidate needs no additional strip compaction.

Personally inspected full-layout and F09/F04/F01 close-up browser renders before and after closure.
The pool pair shares its single connecting pipe cell. The surplus exits below their other product
route instead of crossing it. Full-layout inspection still shows a substantial inter-bank void and
long commodity lanes inherited from the 53-facility composition. Reasonable final space quality is
not yet established. Next: build connected supplier/consumer clusters as compound regions and compare
two-dimensional packing/local interface relocation with the current single-bank arrangement, while
preserving a complete validated incumbent. Do not return to unvalidated global facility-only placement.

## Timing and evidence retention

Node v26.5.0; existing optimized Rust witness probe, no rebuild/migration required. Timings below are
single-run observations, not statistical medians. The stage begins with the already constructed
53-facility witness; do not describe these as full from-scratch factory timings.

| Version | Generation (ms) | Closure CLI elapsed (ms) | Area | Tiles |
| --- | ---: | ---: | ---: | ---: |
| Initial working export family | 3,334.69 | 5,882.80 | 12,900 | 2,455 |
| Exact score pruning and tight export | 1,303.72 | 3,937.42 | 12,900 | 2,401 |
| Selected-state detail/HTML output | 1,279.26 | 2,813.08 | 12,900 | 2,401 |

Template and port-pair pruning uses a sound lexicographic lower bound: used area, endpoint Manhattan
distance plus one cell, and zero turns. A lower bound that cannot beat the incumbent cannot conceal
a better path in that tested family. This says nothing about unenumerated geometry. The early pruned
run retained the exact same final witness hash as the unpruned version. Tightening the output envelope
then intentionally changed the output path. The output-IO cutover preserved all fifty candidate witness
hashes exactly compared with the tight-envelope baseline.

The final CLI saves every candidate witness, independent report and graph coverage, but generates
duplicate detail JSON and HTML only for the seven selected growth/final states. Selected rendering
and detail IO totals 104.10 ms; candidate input/coverage IO totals 975.25 ms. Child validator wall times
overlap in batches of four and must not be summed as elapsed time. The full report separates these
costs from generation. stdout is one short completion summary, never the full witness or trace.

## Verification

- All 59 members, 131 recipe terminals and seven actual external obligations checked against the
  normalized original graph; no unresolved cuts. Both final witnesses independently pass.
- Eight negative/positive audit expectations pass: undeclared external substitution, omitted export
  network, broken pipe segment, duplicate facility, changed final utilization, omitted terminal report,
  inflated terminal report and unchanged complete factory. The substituted external and omitted-export
  cases deliberately pass geometry but fail original-graph completeness.
- 43 related Node tests pass; the new three cover mixed frontier selection, retained-interior identity,
  actual external identities, detached utilization DTOs, layer/body/reservation rules, direct shared
  cells and rejection of fabricated complete reports. The focused suite was rerun after final changes.
- `cargo check --workspace` passes. `cargo test --workspace`: 524 passed, two existing ignores. No Rust
  source/dependency changes. Logs are under `target/factory-closure-{all-tests,final-tests,cargo-check,cargo-tests}.log`.
- Complete-page browser checks pass wheel zoom, actual pointer-drag pan, Fit and six viewport/theme
  combinations (1400/736/360, light/dark). Full and head-close-up images were personally inspected.
- Self-review strengthened the coverage gate to reject missing/duplicate/unexpected terminal rows and
  mismatched totals, detached newly assigned utilization objects, and removed an origin-zero extent
  assumption. This slice introduces no custom propagator or pruning of the production exact solver.

Repository policy comparison: production remains unchanged; this is an isolated JS research stage.
It preserves the accepted valid state, uses runtime game data, validates each connected new input
transaction, and reports bounded local failure without global infeasibility claims. No deferred global
routing phase, CP/SAT fallback or caller canvas bound was introduced. The full goal remains active:
space quality, additional recipes, cleanup and the actual recipe/rate web workflow are unfinished.

## Reproduce and artifacts

Regenerate the prior cyclic blocks and 53-facility composition if their selected witness directories
are absent; see the neighboring composition report. Then use a fresh output directory:

```sh
node docs/benchmarks/constructive-factory-closure/run.mjs \
  --graph docs/benchmarks/constructive-chain-blocks/gap-1-placement.json \
  --baseline docs/benchmarks/constructive-block-composition/matrix.json \
  --recipes data/game/normalized/recipes.json \
  --facilities data/game/normalized/facilities.json \
  --items data/game/normalized/items.json \
  --transports data/game/normalized/transports.json \
  --components data/game/normalized/logistics-components.json \
  --validator target/release/aic-witness-probe \
  --output-directory target/factory-closure-repeat
```

Expected exit 0 and `complete-graph-validated`. `audit.mjs` uses the same flags, except `--baseline`
points to this completed closure's `report.json`. Final frozen evidence is `matrix.json` and
`controls.json`; detailed candidate witnesses remain in their referenced target directories.
The tailnet page is `http://100.114.24.86:8765/bound-interconnect-closure-complete.html`; corresponding
PNG and `-head.png` are in `target/shared-results`. The existing server and hosting setup are unchanged.
