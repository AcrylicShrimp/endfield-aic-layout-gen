# Layout generation Monte Carlo survey

## Result

The new compact survey evaluated all 768 requested slots: 64 slots in each of
12 provisional facility-count buckets. It produced 724 valid layouts and 44
bounded failures in 174.87 seconds on an Apple M4 with four release-mode workers.
No slot was interrupted or omitted. The results establish useful conditional
spatial-occupancy ranges through 79 facilities. They do not support treating the
current `80-100` generator bucket as a reliable equal-weight training source.

This work implements the
[accepted survey design](../designs/2026-09-16.00-layout-generation-monte-carlo-survey.md).
`aic-layout-gen survey` loads and validates catalogs once, retains one generator
per bucket, interleaves bucket work under a wall-time deadline, and writes one
compact row for every requested slot. It stores no full layout or gallery. Rows
retain success metrics or bounded failure diagnostics, attempt count, last
attempt state and total routing work. Summaries retain yield, cost, failure
reasons and occupancy quantiles. Normal generation behavior is unchanged.

## Reproduction

The tracked configuration is
`crates/aic-layout-gen/examples/survey.json`. It assigns 64 slots to every bucket,
uses two construction attempts per slot, one directed connection per facility,
a 10,000,000-expansion per-attempt routing limit and a 15-minute wall-time bound.
Connection topology, facility choice, rotation, placement, ports and transport
routes remain randomized nuisance variables.

```sh
cargo build --release -p aic-layout-gen
target/release/aic-layout-gen survey \
  --config crates/aic-layout-gen/examples/survey.json \
  --catalog-dir data/game/normalized --seed 20000 \
  --output artifacts/layout-generation-survey/monte-carlo-20260916
```

The retained ignored report is
`artifacts/layout-generation-survey/monte-carlo-20260916/report.json` (748,523
bytes). Its catalog identity is
`6e483b5b903fe759008823f50dfe55d6e9a7b3d8d36978864fb292a76d5684de`.
Timing is observational and worker scheduling does not affect slot seeds. A slot's
seed is the base seed plus its bucket-major row index with `u64` wrapping.

## Bucket yield, cost and occupancy

Occupancy is `(body cells + unique transport cells) / bounding-box area` from
independent physical validation. The table reports the 5th, median and 95th
percentiles among successes. Mean time and work include failures. Quantiles use
the report's documented lower-rank selection and are descriptive, not final
training boundaries.

| Facilities | Success | Yield | Mean ms/slot | Mean routing work | Occupancy p05 | p50 | p95 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 2-3 | 63/64 | 98.4% | 0.34 | 869 | 0.462 | 0.598 | 0.715 |
| 4-5 | 64/64 | 100.0% | 0.47 | 1,852 | 0.377 | 0.467 | 0.588 |
| 6-7 | 63/64 | 98.4% | 0.89 | 3,090 | 0.351 | 0.435 | 0.567 |
| 8-11 | 64/64 | 100.0% | 2.19 | 7,483 | 0.336 | 0.441 | 0.558 |
| 12-15 | 64/64 | 100.0% | 5.72 | 19,916 | 0.348 | 0.431 | 0.511 |
| 16-23 | 64/64 | 100.0% | 16.98 | 59,003 | 0.328 | 0.415 | 0.546 |
| 24-31 | 64/64 | 100.0% | 77.88 | 265,625 | 0.356 | 0.430 | 0.523 |
| 32-39 | 64/64 | 100.0% | 241.99 | 831,539 | 0.387 | 0.444 | 0.528 |
| 40-49 | 64/64 | 100.0% | 605.16 | 2,014,496 | 0.378 | 0.460 | 0.540 |
| 50-63 | 64/64 | 100.0% | 1,299.79 | 4,288,853 | 0.403 | 0.466 | 0.538 |
| 64-79 | 62/64 | 96.9% | 2,799.49 | 9,098,152 | 0.410 | 0.478 | 0.534 |
| 80-100 | 24/64 | 37.5% | 5,760.67 | 17,846,094 | 0.454 | 0.497 | 0.540 |

Two small-bucket failures ended at pair-trial exhaustion. Two `64-79` failures
and all 40 `80-100` failures ended at the routing-work limit. Of 768 slots, 685
finished on the first attempt and 83 used both attempts. The generator therefore
has adequate bounded yield through 79 under this configuration, while routing
cost rises sharply from 40 facilities onward.

Across all 724 successes, occupancy ranged from 0.266 to 0.827. The 5th, 25th,
50th, 75th and 95th percentiles were 0.354, 0.417, 0.459, 0.501 and 0.560.
Those pooled values hide the facility-count relationship: tiny layouts have the
widest and highest tail, while the median rises gradually from about 0.42 at
`16-23` to 0.50 in the selected `80-100` successes. Conditional bucket ranges
must therefore replace one global hard occupancy range.

The broad final bucket also shows acceptance bias. Its 24 successes contained
2 layouts at 80 facilities and examples through 95, but none at 96-100. Because
each retry resamples a size inside the bucket, this does not measure fixed-size
yield and cannot justify excluding the upper end.

## Fixed high-count investigation

A second bounded run held each requested size fixed at 80, 85, 90, 95 or 100,
with 16 slots per size and the same generator budgets. It completed all 80 slots
in 116.34 seconds and produced 18 successes:

| Facilities | Success | Yield | Mean ms/slot | Failure reason |
| ---: | ---: | ---: | ---: | --- |
| 80 | 9/16 | 56.3% | 4,355.7 | route-work budget (7) |
| 85 | 6/16 | 37.5% | 5,432.0 | route-work budget (10) |
| 90 | 2/16 | 12.5% | 5,988.0 | route-work budget (14) |
| 95 | 1/16 | 6.3% | 6,272.6 | route-work budget (15) |
| 100 | 0/16 | 0.0% | 6,310.0 | route-work budget (16) |

The exact-size configuration was derived from the tracked survey configuration:

```sh
jq '.facility_buckets=[[80,80],[85,85],[90,90],[95,95],[100,100]] |
    .attempts_per_bucket=16 | .max_elapsed_seconds=600' \
  crates/aic-layout-gen/examples/survey.json \
  > artifacts/layout-generation-survey/high-exact-config.json
target/release/aic-layout-gen survey \
  --config artifacts/layout-generation-survey/high-exact-config.json \
  --catalog-dir data/game/normalized --seed 30000 \
  --output artifacts/layout-generation-survey/high-exact-20260916
```

Zero successes at 100 is bounded evidence about this generator and budget, not a
proof of infeasibility. Equal-weight training through 100 needs a routing/generator
improvement or an explicitly accepted large cost and selection bias. The high
bucket must not be silently removed or replaced by easier sizes.

## Distribution decision supported by the evidence

The original facility bucket boundaries remain useful through `64-79`, and each
can receive equal requested weight. A provisional occupancy policy can use each
bucket's measured p05-p95 interval above as its common-support region while
retaining the observed tails for explicit robustness sampling. One global density
interval would overrepresent unusual tiny-layout density or reject normal large
layouts.

The final `80-100` facility policy and the weighting of occupancy tails remain
product decisions. No training configuration was changed and no new pretraining
run was started. The implementation preserves these questions for agreement
rather than encoding an unsupported answer.

## Verification

```sh
cargo fmt --all
cargo check --workspace
cargo test --workspace
node docs/maintenance/check-tracked-files.mjs --worktree
```

The CLI integration check verifies equal slots, deterministic bucket-major seed
identity, complete report publication and absence of full layout files. The
release-mode survey itself exercised all 768 primary slots and 80 fixed-size
follow-up slots. Generated reports remain under ignored `artifacts/`.
