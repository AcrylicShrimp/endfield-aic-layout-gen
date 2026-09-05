# Fixed flow orientation experiment

## Outcome

Rotation locking plus aligned-face port preference improves retained Heavy Xiranite coverage, but
does not complete the factory. This is not the user-requested direct-neighbor placement algorithm.
Keep this opt-in and preserve the complete default constructor.

| Mode | Facilities / 59 | Area / tiles / turns | Construction seconds | A* calls |
| --- | ---: | --- | ---: | ---: |
| Default baseline | 59 | 23100 / 4265 / 298 | 3.316 | 178 |
| Candidate, options disabled | 59 | 23100 / 4265 / 298 | 3.149 | 178 |
| Downstream-closed coarse | 30 | 5913 / 659 / 53 | 0.425 | 182 |
| Closed coarse, fixed east | 54 | 18487 / 2641 / 202 | 124.047 | 1646 |
| Downstream-closed nearby 4 | 36 | 2700 / 1040 / 84 | 12.850 | 1266 |
| Closed nearby 4, fixed east | 42 | 3927 / 1428 / 149 | 51.653 | 6398 |

Single release samples, not timing medians. All six small-cycle runs complete. Every retained
closed-mode snapshot has zero missing internal-output requirements; fixed-mode placements match
their preflight absolute orientation. These partial footprints are not quality wins against a
complete factory. Catalog-mixed faces remain possible, and this does not constrain route monotonicity.

Fixed coarse's final failed transaction considers 2,731,802 attachment anchors and rejects 1,539
completed routes for future-boundary dead ends. Its 1,551 A* searches all find routes: this is not
primarily a no-path failure. Exact boundary/attachment repair remains unimplemented. Nearby's
different failure must not be explained by the earlier furnace/grinder overlap without evidence.

## Reproduction and evidence

Run `AIC_FLOW_EXPERIMENT=1 node docs/benchmarks/constructive-downstream-order/run.mjs
BASELINE_BINARY CANDIDATE_BINARY NEW_OUTPUT_DIRECTORY`, using parent `2c5edb8` as baseline.
`matrix.json` records source/input/binary hashes, unchanged-witness checks, work, failure details
and operation caps. Full generated history resides in `/tmp/aic-fixed-flow.zWmJDu/matrix/` locally;
separate fixed-east HTML copies are served from ignored `target/shared-results/` on the existing
tailnet server. No previous artifact is replaced.

Verification: `cargo fmt --all`, `cargo check --workspace`, `cargo test --workspace` (505 passed,
two existing ignores), release CLI build and twelve CLI/report audits. No build/test overlap during
measurement. No wall guard fired. No dependency or exact solver changes.

## Next decision

Implement the user's connection-first placement: generate positions only from already placed direct
neighbors, choose the proper flow side, minimize connection distance, then route and locally retry.
Existing coarse candidates rotate around a single anchor; nearby candidates surround every facility
and rank area first. Neither implements that algorithm. This is a construction proposal heuristic,
not a change to committed improvement score priority. A restricted local CP oracle may be evaluated
later; neither this result nor a restricted failure justifies global exact feasibility claims.
