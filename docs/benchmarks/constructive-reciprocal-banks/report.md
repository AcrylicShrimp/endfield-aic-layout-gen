# Adjacency-constrained reciprocal bank reconstruction

## Outcome

The new complete incumbent is **47x61 = 2,867 area, 426 transport cells, 38 turns**.
All 59 facilities and the original production obligations pass the independent validator
and all eight original-graph audit controls. The audit's graph and witness hashes match
the selected artifacts. The previous complete incumbent is preserved at
`../constructive-interconnect/compact-results.json`: 3,120 / 868 / 106.
Area improves 8.11%; transport cells improve 50.92%. This includes changed allocation,
packing and local refinement, not an isolated crossing-geometry effect.

See `results.json` for machine-readable scores and artifact paths. Detailed trial data
remain in `target/`; historical compact-crossing results are not overwritten.

## Mechanism and measured limits

Discover the reciprocal family from runtime recipe inputs/outputs, not facility labels.
Keep seed-return allocations fixed. Solve an exact rational forward-flow allocation
whose internal arcs can connect adjacent facilities. Preserve every producer's output
and every consumer's input, including ordinary downstream processors. Jointly assign
actual catalog ports and immediately connect the selected internal edges.

The successful strip orders F32, F45, F33, F46, F38 with alternating plant/collector
orientations. It replaces the nonlocal F32-to-F46 plant allocation by redistributing
plant supply among existing consumers without changing utilization or rates. The five
facility core uses 175 area, 12 belt cells and no turns. Both supported east-facing core
variants have independently valid input-closed prefixes of 2, 4 and 5 facilities.
These are private-bank projections, not production frontier event traces.

Attach F23/F26 through the existing shared composition engine. The seven-facility bank
improves from 910 / 301 / 60 to 350 / 50 / 8. All seven unrelated banks are independently
rechecked and their original witnesses retained. Four exported shape variants share
the same exact allocation. Other allocation variants are not silently mixed.

The bounded enumeration visits 384 layouts, rejects 376 for nonadjacent retained return
edges, and physically validates all eight remaining cores. Two have the currently
supported east-facing interface; eight composed alternatives validate. Generation takes
0.320s and the bank runner including validation/artifacts takes 1.622s on the final run.
The earlier run used by whole-factory construction has identical graph and selected
witness hashes. Detail JSON differs because it contains run-specific paths.

## Complete-factory comparison

Twelve shape packings with two closure policies produce 14 complete trials out of 24,
across ten distinct packings. The previous crossing-only matrix completed two trials
from one packing. This is descriptive evidence, not a statistical success probability.
The best raw new factory is 3,773 / 640 / 78 and does not replace the incumbent yet.

Three group/A* improvement rounds reach 2,867 / 426 / 38, terminating with no improvement
in the configured neighborhood (128 selected positions, terminal-flexible endpoints).
A different wider seed reaches 3,510 / 679 / 53 and is not selected. The 24-trial whole
construction matrix takes 52.24s; the selected refinement takes 30.61s and the alternate
40.94s. Concurrent execution occurred. These are experiment-batch timings, not a claim
about single-factory latency or an isolated performance benchmark.

## Visual inspection and next bottleneck

Personally inspected the native five-facility core, seven-facility bank, complete factory
and crossing close-up. Local crossing routes now fit beside consumers rather than
requiring repeated exchange-stage loops. The final group refinement can break the
initial touching strip when the complete area benefits; it does not promise to retain
the core's twelve-cell geometry in the full factory.

The largest remaining visible void lies below F00 and above the lower F03 chain. The
largest empty rectangle is 21x8 = 168 cells. Occupied XY is 1,533 and empty XY is 1,334
(46.53%), versus 1,191 empty cells in the previous larger factory. Removing detours can
reduce both total area and occupied fill: fill alone must not select the winner.
There is also a small folded lead near the F38/F26 boundary. No minimum-route or
minimum-area claim is made for the composed bank or final factory.

Next compare coupled movement of the product-side chain and its enclosing pipe/belt
approaches with alternate source-bank boundaries. Moving only one facility or further
contracting crossing cells has exhausted this configured neighborhood. This is not proof
that the void is unavoidable. SCC stop/grow partitioning, broader recipe coverage and
product integration remain open.

## Reproduction and verification

Run `run.mjs` with `--graph target/scc-stop-grow-final/graph.json`,
`--blocks target/reciprocal-direct-attached/blocks.json`, `--block cycle-7`, the five
catalog flags pointing to `data/game/normalized/`, `--validator target/release/aic-witness-probe`
and a fresh `--output-directory`. Inputs and shared implementation hashes are recorded
by the runner. The emitted graph and blocks feed the existing cluster-packing runner
with `--source-shapes <new-blocks.json> --shape-limit 12 --shape-beam 8
--packing-family nesting --placement-family two-sided`.

Refine packing 11/simple and packing 0/simple independently using their closure reports,
`--rounds 3 --mode joint --move-family group --move-limit 128
--endpoint-policy terminal-flexible`. Audit the selected result using `audit.mjs` and
the original `constructive-chain-blocks/gap-1-placement.json`, not just the reallocated
graph. See the exact report paths in `results.json`.

- 137 Node tests pass across fourteen constructive research suites, including three new
  tests for exact margins, label-independent discovery and all eight real core candidates.
- `cargo check --workspace` passes; `cargo test --workspace` passes 524 tests with two
  existing ignores. No Rust code changed.
- Eight original-graph controls pass, including deliberate capacity/coverage/geometry
  corruption. The selected witness hash matches the unchanged control.
- Native browser checks pass for six viewport/theme combinations, pan, zoom and Fit.

Policy review: the experiment remains isolated JavaScript research, uses runtime catalogs,
has no production W/H limit or global solver fallback, retains unrelated validated banks,
and only promotes a fully validated whole factory by area/cells/turns. It does not change
the immediate-routing Rust constructor described in
`docs/designs/2026-09-02.00-constructive-layout-planner.md`.
