# Local randomized-start spatial diversity

The paired local probe completed all twelve setups on four newly generated
requests in 7.656 seconds. Every final current layout passed native admission
and exactly matched the admitted export. Existing distant/group vocabularies
produce larger spatial changes cheaply, but leave substantial constructor row
and column structure. This is not evidence of uniform valid-layout sampling.
Normal generation and the concurrent server learner were not changed.

The [registered design](../designs/2026-09-13-generator-spatial-diversity-probe.md)
compares the current 2–6 local edits against sixteen distant or group edits.
The same actual constructor witness and setup seed are reused within each
request; no outcome selects requests or exports. Seeds are the first occurrence
of each configured product in a predeclared seed interval. Rates and facility
counts come from external game data and the existing generator.

| Product | Seed | Rate | Facilities | Local / distant / group setup seconds |
| --- | ---: | --- | ---: | --- |
| Food | 613700000 | 1 / 35 s | 23 | 0.099 / 0.238 / 0.345 |
| Watered grass | 613700001 | 1 / 12 s | 2 | 0.012 / 0.052 / 0.057 |
| Medical | 613700006 | 1 / 55 s | 18 | 0.073 / 0.194 / 0.246 |
| Copper liquid | 613700017 | 1 / 10 s | 3 | 0.031 / 0.054 / 0.056 |

Setup times include runtime admission through final independent admission inside
the worker's timed interval, but exclude worker import/startup and original
construction. The whole-matrix time includes constructor and subprocess costs.
There were no rejected factory generations, failed setups or time-limit exits.
Some candidate edits were rejected; these retained the current valid state.

## Spatial evidence

Mean pair-offset change is the mean Manhattan difference between every pair's
relative facility origins and the constructor pair, in tiles. It ignores global
translation. Axis alignment is the share of facility pairs sharing an exact
origin x or y; it detects some regularity, not every grid pattern.

| Product | Mean pair-offset change: local / distant / group | Changed rotations: local / distant / group |
| --- | --- | --- |
| Food | 2.356 / 10.656 / 15.573 | 1 / 3 / 0 |
| Watered grass | 1 / 21 / 27 | 0 / 2 / 2 |
| Medical | 0.529 / 9.791 / 14.608 | 1 / 3 / 1 |
| Copper liquid | 2 / 18 / 12 | 1 / 3 / 2 |

For food, aligned-pair share changes from 15.42% after local setup to 13.44%
after distant setup and 11.86% after group setup. For medical, it changes from
18.30% to 13.07% and 18.95%. Medical group setup retains the same nine distinct
x origins and eight y origins as local setup, despite larger displacement.
The recorded large-factory plot also shows that most row/column patterns remain.

All twelve outputs have changed geometry hashes, but that fact alone would miss
the remaining regularity. Counts of changed normalized facility origins can be
inflated by moving the extremal facility, so pair offsets and the visual evidence
are needed alongside them. Two/three-facility cases provide weak evidence about
larger spatial distributions.

Footprints vary in both directions. Medical local/distant/group areas are
4,662 / 4,725 / 5,328; food areas are 4,898 / 3,596 / 4,087. Current exports were
retained even when worse than the starting state or an earlier best. This avoids
turning generator diversity into accidental score-based selection.

This first probe changes both edit count and vocabulary, so it does not isolate
the contribution of distant movement from additional steps. There is one draw
per product. It establishes a cheap available perturbation mechanism and its
visible limits, not a distribution ranking or training benefit.

## Decision and continuity

Keep the current main generator and completed 80/10/10 curriculum unchanged.
The next construction-diversity slice should test randomized local growth choices
(direction, spacing, permitted rotation and branch placement), while retaining
incremental routing, transactional validation and complete final factories.
Larger valid perturbations can remain a candidate diagnostic arm; they have not
been adopted as a normal training distribution.

The continuity checklist is closed for this diagnostic: actual fresh requests,
unchanged incremental constructor, external runtime data, final-current identity,
complete native validity, explicit bounded failure handling and separation from
the server launch were exercised. No new production API or game rule was added.
Verification is the actual twelve-arm CLI workflow plus inspection of its saved
plot; no model training, native proof replay or new Rust tests were required.

## Reproduction and retention

```sh
tools/factory-edit-pilot/.venv/bin/python \
  tools/factory-edit-pilot/diagnostics/generator_spatial_diversity.py run \
  --native-library target/release/libaic_pilot_native.dylib \
  --output artifacts/generator-spatial-diversity/local-v1
tools/factory-edit-pilot/.venv/bin/python \
  tools/factory-edit-pilot/diagnostics/plot_generator_spatial_diversity.py \
  --root artifacts/generator-spatial-diversity/local-v1 \
  --output artifacts/generator-spatial-diversity/local-v1/comparison.png
```

The explicit configuration is
`tools/factory-edit-pilot/diagnostics/generator-spatial-diversity.json`.
Use `.so` on Linux. Plotting uses the existing local Matplotlib environment and
only reads retained witnesses. The ignored output keeps actual requests,
constructor witnesses, per-arm tasks/current layouts, native admission reports,
compact metrics and the comparison plot. Generated output is not tracked source.


## User decision after inspection

The user found that these perturbed layouts still lack human-like dense related
facility groups connected across regions. Coordinate displacement is not an
adequate proxy for that preference. The user then reconsidered the deployment
contract: real inputs are the current constructor's outputs plus the model's own
edits, because the product first constructs a factory and then compresses it.
That distribution is sufficient for the present goal. Keep the current generator
and restored 80/10/10 curriculum. Close this diagnostic without adopting its arms
or proceeding with the proposed human-like/constructor-randomization follow-up.
