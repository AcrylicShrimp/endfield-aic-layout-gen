# First connected dynamic-factory training result

Status: measured; not production-ready and no reliable learned advantage established.

Historical schema-1 source/config: commit `8d317f5`. Current training accepts schema 2;
the subsequent temporal-credit run and explicit migration are in `credit-report.md`.

## Outcome

The main path now generates new catalog-derived production requests, constructs complete
factories using the normal transactional Rust constructor, trains actual multi-step direct
proposals, and evaluates untouched target-family development requests. This replaces neither
the production CLI nor the release gates. The old fixed-root experiment is an archived baseline.

The run completed in 488.88 seconds: 64 optimizer updates, 65,536 selected training actions,
56 new training factories and eight explicit Heavy Xiranite replay groups. Each group used
four independent 256-step trajectories. There were 56 informative terminal-comparison groups
and 28,967 changed weight elements out of 29,537 parameters. Checkpoint reload reproduced the
same direct action and exact log probability. These establish execution, not useful learning.

## Independent development controls

All models started from the same 12 newly generated validation roots with per-root random
streams. The 59-facility Heavy Xiranite replay target was additional, not held out. All 78
evaluations completed their full 256/1,024 proposals. All 256 training episodes and 78 evaluation
outputs passed fresh geometry and original-production coverage checks. The low-level geometry
validator's `factory_validated: false` is expected: full request coverage is a separate check.
No sealed test was queried.

| Proposals | Policy | Mean retained / initial area | Futile proposals | Trained vs policy: wins / ties / losses |
| --- | --- | --- | --- | --- |
| 256 | Initial | 0.205309 | 78.91% | 5 / 3 / 4 |
| 256 | Uniform | 0.204843 | 79.04% | 3 / 4 / 5 |
| 256 | Trained | 0.208843 | 79.46% | — |
| 1,024 | Initial | 0.190849 | 81.70% | 4 / 3 / 5 |
| 1,024 | Uniform | 0.192249 | 81.88% | 6 / 2 / 4 |
| 1,024 | Trained | 0.188700 | 81.63% | — |

Wins use the exact area, physical-transport tiles, turns tuple, not area alone. Large reductions
from loose initial construction occur for every control; they are not evidence of learning.
One training seed and one evaluation seed are insufficient for a method-effectiveness claim.
Uniform means uniform at each conditional choice, not uniform over all complete joint actions.

Heavy Xiranite remains `[2132, 392, 147]` with **29.6904% empty XY area** for all six controls.
At 1,024 proposals, its trained futile rate is 84.375%, versus 84.180% initial and 84.766% uniform.
Seven of eight target training groups have no task comparison signal; one trajectory in group
48 reduces tiles by one without reducing area. There is no new target geometry worth a new
density claim; the earlier baseline visual audit still applies. The 1% gate remains unmet.

## What the stream actually covers

257 attempts produced 99 complete factories, 143 material-flow rejections, nine duplicate
production fingerprints, four exhausted constructions and two construction timeouts. The
31 accepted one-facility factories were explicitly learning-ineligible, not construction failures.
No rejected request was cropped, replaced by a saved root, or given new external inputs to rescue
geometry. Procurement choices were sampled before geometry under the stated policy.

The 56 new training factories cover 44 target items and 87 selected recipes; 22 use both belt
and pipe. Facility counts are 24 two-facility, 13 three-facility, 10 four-facility, five
five-facility, and one each with six, seven, eight and ten facilities. The 12 validation factories
cover eight target items and reach 18 facilities. Source-cycle references occur in 22 training
requests and three validation requests; this is not a solved-active-SCC census.

This is a genuinely generated stream, but it is strongly small-factory biased. It has no new
17–64 or 65–128-facility training exposure. A 128-instance safety guard is not evidence of
coverage to that size. Three four-facility seed targets exhaust growth; validation battery
requests exhaust placement or the five-second constructor deadline. These remain measured
failures, not global infeasibility proofs. See `results.json` for every constructor failure.

## Executor and measurement limitations

- The model emits connected members, catalog rotation/port and integer XY parameters without
  enumerating complete candidate mutations. Body masks cover every permitted position in the
  current executor extent; routing and scores are not used to rank mask entries.
- Execution still freezes external anchors and forbids extent expansion. These are scoped
  kernel limitations, not game bounds. No general layout reachability claim follows.
- The CNN receives exact layer occupancy but aggregate direction channels; intersecting arms
  can overwrite direction at a junction. Full networks remain in the DTO but are not all
  consumed by the policy. Component token dimensions use the older feature encoder convention.
- Semantic fingerprints include source-path identities. They are stronger than serial root IDs
  but do not prove graph-isomorphism deduplication across arbitrary renamings.
- Controls share selected-action and per-action routing limits, not consumed routing work.
  Model masking/inference costs are included in episode wall time, but this version failed to
  retain their separate evaluation-batch measurement. Environment preprocessing alone is not
  total preprocessing. Co-batched wall times are not isolated inference latency measurements.
- Python/worker source hashes were checked at shutdown. New analysis/tests/docs were added only
  after the run. Existing user-owned closure/view changes were used as recorded inputs, not
  committed or changed by this slice.

## Decision and next main-path work

Do not extend operator-capability pilots or call this trained model effective. Improve the
connected learner: specify quality-preserving credit for avoiding futile actions, then measure
it against the recorded controls with longer actual trajectories. Broaden rate/production
sampling and report real constructor acceptance by requested size. Keep original production
coverage, target-family holdout and failed attempts visible. Fix evaluation inference accounting.
Any larger-factory construction work must remain connected and transactional, not facility-only
placement followed by deferred routing. No production integration until release gates pass.

## Verification and reproduction

```sh
uv run --frozen --project docs/benchmarks/layout-learning python docs/benchmarks/layout-direct/train.py \
  --plan docs/benchmarks/layout-direct/sampling.json --config docs/benchmarks/layout-direct/train.json \
  --output-directory target/layout-direct-train-01
node docs/benchmarks/layout-direct/analyze.mjs \
  --report target/layout-direct-train-01/report.json --output artifacts/benchmarks/layout-direct/results.json
node --test docs/benchmarks/layout-direct/*.test.mjs
uv run --frozen --project docs/benchmarks/layout-learning python -m unittest discover \
  -s docs/benchmarks/layout-direct -p 'test_*.py'
cargo check --workspace
cargo test --workspace
```

Use fresh output paths when reproducing; artifacts are intentionally not overwritten. Ten Node
tests and four Python tests pass, including exact replay gradients, exhaustive body-mask checks,
valid-worse delayed improvement, compound-phase validation, late-result rollback and analyzer
audit checks. Workspace check/tests pass. No Rust code changed, so formatting was not needed.
The harmless Torch missing-NumPy warning remains; this code does not use NumPy.

Architecture review against the accepted constructive design and AGENTS policy: normal
connected, immediately routed production construction; independent original-request validation;
pipe-first local repair; no exact-solver fallback; immutable retained best in score order;
private valid-worse rollouts do not become production commits. What became simpler is the new
worker's action boundary: explicit operation parameters instead of a prebuilt mutation list.
The old experiment remains isolated evidence, not a compatibility route in this new worker.
