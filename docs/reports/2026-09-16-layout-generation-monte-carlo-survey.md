# Scale-aware layout generation survey

This is a baseline measurement of the abstract physical generator. Subsequent
[prior-art review](2026-09-16-random-factory-generation-prior-art.md) found that
its random placement, routability-conditioned edge construction and irreversible
sequential routing are not an adequate random-factory architecture. The results
below remain valid for the measured implementation but do not justify using its
outputs as a training distribution.

## Result

The final semantic generator completed all 768 requested raw slots in 508.60
seconds on an Apple M4 with four release-mode workers. It produced 622 validated
layouts and 146 bounded failures. No slot was interrupted, omitted, retried or
replaced. Every row retained its sampled facility count, topology, packing and
long-connection preference.

The scale policy restored useful one-attempt support at the high end. Raw yield
was 60.9% for 50-63 facilities, 57.8% for 64-79 and 59.4% for 80-100. The final
bucket contained successes through exactly 100 facilities. These are conditional
generator measurements, not a selected training distribution.

The earlier mechanism-level survey is superseded for distribution decisions. It
used a fixed edge-density parameter, two attempts that could resample the exact
request, one packing range and caller-maintained work limits. Its high-count
selection bias and occupancy ranges do not describe the semantic generator.

This report implements the accepted
[scale-aware generator design](../designs/2026-09-16.01-scale-aware-layout-generator.md)
and replaces the conclusions previously recorded at this path.

## Semantic and evidence contract

The `aic-layout-generator-v2` public configuration samples intent:

- an exact facility count;
- `tree`, `light-cycles` or `branching` topology;
- `compact`, `balanced` or `spacious` packing;
- a long-connection construction preference; and
- an effort tier plus game-facing transport, crossing and acyclic constraints.

`scale-policy-v1` resolves mechanism limits from that fixed request. Route-call
work grows with facility count squared and with packing profile; total attempt
work also grows with target connections. Reports retain every resolved value.
Production retries change only construction randomness. The survey forces one
attempt so it measures raw support without hiding difficult requests behind
retry selection.

Long-connection preference is intentionally not an accepted-edge fraction. A
preferred trial selects only pairs in the upper quartile of block-origin
Manhattan distance; an ordinary trial selects the lower three quartiles. Routing
feasibility determines which trials become edges. Attempts record the distance
threshold and accepted long-edge count.

The final 768 rows passed the release audit: every row had one attempt and a
resolved one-attempt policy, every facility count remained inside its requested
bucket, every topology target matched its scale formula, and every success had
exactly the requested number of connected edges. All layouts passed independent
physical validation before admission.

## Reproduction

The tracked configuration is
`crates/aic-layout-gen/examples/survey.json`. It gives 64 raw slots to each of 12
facility buckets, samples all nine topology/packing combinations, uses standard
effort and has a 15-minute wall guard.

```sh
cargo build --release -p aic-layout-gen
target/release/aic-layout-gen survey \
  --config crates/aic-layout-gen/examples/survey.json \
  --catalog-dir data/game/normalized --seed 43000 \
  --output artifacts/layout-generator-v2/monte-carlo-final-20260916
```

The ignored report is
`artifacts/layout-generator-v2/monte-carlo-final-20260916/report.json` (1,311,780
bytes). Its catalog identity is
`6e483b5b903fe759008823f50dfe55d6e9a7b3d8d36978864fb292a76d5684de`.
Slot seeds are the base seed plus bucket-major row index with `u64` wrapping.
Worker scheduling affects timing only.

## Yield, cost and occupancy by facility count

Occupancy is `(body cells + unique transport cells) / bounding-box area` from
independent validation. Quantiles use the report's documented lower-rank
selection among successes. Mean time and routing work include failures.

| Facilities | Success | Yield | Mean ms/slot | Mean routing work | Occupancy p05 | p50 | p95 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 2-3 | 52/64 | 81.3% | 0.2 | 667 | 0.336 | 0.593 | 0.800 |
| 4-5 | 60/64 | 93.8% | 0.7 | 2,140 | 0.231 | 0.482 | 0.683 |
| 6-7 | 64/64 | 100.0% | 1.2 | 3,674 | 0.212 | 0.419 | 0.610 |
| 8-11 | 64/64 | 100.0% | 4.4 | 11,980 | 0.227 | 0.426 | 0.658 |
| 12-15 | 64/64 | 100.0% | 16.1 | 41,848 | 0.203 | 0.408 | 0.685 |
| 16-23 | 61/64 | 95.3% | 68.0 | 175,814 | 0.226 | 0.457 | 0.696 |
| 24-31 | 51/64 | 79.7% | 245.9 | 596,846 | 0.211 | 0.431 | 0.689 |
| 32-39 | 52/64 | 81.3% | 628.6 | 1,562,363 | 0.254 | 0.436 | 0.672 |
| 40-49 | 40/64 | 62.5% | 1,512.3 | 3,621,508 | 0.222 | 0.441 | 0.696 |
| 50-63 | 39/64 | 60.9% | 3,608.6 | 8,278,431 | 0.250 | 0.536 | 0.692 |
| 64-79 | 37/64 | 57.8% | 7,498.7 | 17,584,175 | 0.295 | 0.481 | 0.692 |
| 80-100 | 38/64 | 59.4% | 17,878.9 | 41,619,182 | 0.292 | 0.469 | 0.644 |

Across all 622 successes, occupancy ranged from 0.166 to 0.845. Its p05, p25,
p50, p75 and p95 values were 0.233, 0.340, 0.464, 0.590 and 0.696. The wide
pooled interval is expected because packing is now an explicit sampled intent;
one global occupancy band would erase that distinction.

The cost curve remains material. A raw 80-100 slot averaged 17.9 seconds and
41.6 million routing expansions, while 12-15 averaged 16.1 milliseconds and
41.8 thousand. Equal training weight by requested sample would therefore not be
equal in generation cost.

## Profile support

Profile counts differ slightly because the survey samples profiles rather than
stratifying them. Each cell has enough requests to expose the main relationship.

| Topology | Packing | Success | Yield | Occupancy p05 | p50 | p95 |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| tree | compact | 68/82 | 82.9% | 0.494 | 0.597 | 0.682 |
| tree | balanced | 84/91 | 92.3% | 0.323 | 0.425 | 0.542 |
| tree | spacious | 71/76 | 93.4% | 0.193 | 0.243 | 0.369 |
| light-cycles | compact | 72/96 | 75.0% | 0.524 | 0.642 | 0.748 |
| light-cycles | balanced | 72/80 | 90.0% | 0.361 | 0.469 | 0.587 |
| light-cycles | spacious | 58/74 | 78.4% | 0.227 | 0.294 | 0.388 |
| branching | compact | 50/82 | 61.0% | 0.559 | 0.663 | 0.740 |
| branching | balanced | 80/99 | 80.8% | 0.376 | 0.526 | 0.628 |
| branching | spacious | 67/88 | 76.1% | 0.240 | 0.339 | 0.434 |

All nine cells have support. Branching plus compact packing is the hardest cell,
while tree plus balanced or spacious is the easiest. Of the 146 failures, 117
ended at the route-work budget and 29 at pair-trial exhaustion. These are bounded
failures, not infeasibility claims. Standard production generation has four
request-preserving attempts and therefore has a different completion rate from
this raw survey.

## Visual inspection

Visual QA used two controlled same-seed sets under the final policy. The ignored
interactive galleries and SVG/PNG previews are under
`artifacts/layout-generator-v2/visual-qa-final-20260916`.

At 100 facilities, one fixed tree request produced all three packing profiles:

| Packing | Dimensions | Occupancy | Connections | Accepted long | Visual finding |
| --- | ---: | ---: | ---: | ---: | --- |
| compact | 103x95 | 0.674 | 99 | 20 | tightly grouped facilities and dense routing |
| balanced | 137x120 | 0.532 | 99 | visibly larger aisles and intermediate density |
| spacious | 206x191 | 0.338 | 99 | broad empty regions and clearly separated groups |

At 30 facilities with one fixed balanced packing, topology alone changed the
result:

| Topology | Connections | Branch blocks | Merge blocks | Occupancy |
| --- | ---: | ---: | ---: | ---: |
| tree | 29 | 5 | 7 | 0.392 |
| light-cycles | 37 | 11 | 10 | 0.454 |
| branching | 44 | 12 | 14 | 0.469 |

The six images were opened and inspected at full preview resolution. Facility
spacing changed in the intended order, and topology density and junction use
changed in the intended order. Independent validation confirmed body separation,
port use, route continuity and bridge declarations. Large layouts visibly route
many long paths around the perimeter and remain visually busy. They are valid and
diverse generator sources, but they are not polished human factory designs.

## Policy calibration

The first provisional resolver used linear per-call work (`500*n`) and allowed
ordinary trials to choose distant pairs as well. On the same 768 semantic slots it
produced 441 successes and zero successes in every bucket from 50 upward. A
controlled exact-100 matrix showed that multiplying the budget alone still left
most profiles blocked.

The final policy separates distant and ordinary candidate classes, scales route
calls with packing area (`n*n` with packing coefficients), and keeps eight port
pairs per selected facility pair. Trying twelve ports reduced yield because failed
route calls consumed the attempt budget sooner. The final exact-100 calibration
completed 13 of 18 one-attempt cases and had at least one success in every
profile cell. The final full survey then confirmed support rather than relying on
one showcase layout.

## Distribution decision

The generator now supports the full requested 2-100 range without caller-tuned
mechanism limits, so no high-count bucket needs to be silently removed. The
survey also shows that size, topology and packing jointly control yield, occupancy
and generation cost. A final training distribution should therefore specify
requested semantic strata and decide how much generation cost to allocate to the
hard branching/compact and high-count regions.

No training distribution was changed and no pretraining run was started. Those
choices remain a separate review decision.

## Verification

```sh
cargo fmt --all
cargo check --workspace
cargo test --workspace
node docs/maintenance/check-tracked-files.mjs --worktree
```

The release survey, exact-100 calibration and six visual galleries are additional
integrated evidence retained under ignored `artifacts/`.
