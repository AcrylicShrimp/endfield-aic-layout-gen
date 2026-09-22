# Hierarchical route candidate baseline

## Question

Can the first non-learned lower boundary return several distinct complete
routes, replay every returned witness through the native constructive session,
preserve later routability and distinguish a locally exhausted frontier from a
work-limited result at bounded cost?

## Frozen inputs

- Source commit: `ed312566783b641d5a9b28013d047818d4292fff`
- Panel: [`panel.json`](panel.json)
- Panel SHA-256: `0f3bfbc58e3f19ca3926dfe0ef54a4f305ab40f66b1fbdc79f43db079c408556`
- Catalog SHA-256: `6e483b5b903fe759008823f50dfe55d6e9a7b3d8d36978864fb292a76d5684de`
- Host: Apple M4, 10 physical and 10 logical CPUs, Darwin 25.6.0 arm64
- Workers: 4 persistent Rayon workers
- Measurement: 500 repetitions of four held-out cases, including downstream
  candidate generation after every root candidate
- Sample: one complete route candidate accepted by native replay

The cases cover an open route, a horizontal route followed by a crossing
vertical obligation, two capacity-compatible same-item branches, and a sealed
fixed request. This is a small contract panel rather than a representative
factory distribution.

## Command

```sh
cargo build --release -p aic-cli --bin aic-cli --features outcomes
target/release/aic-cli learning route-candidate-harness \
  --panel docs/benchmarks/hierarchical-route-candidates/panel.json \
  --catalog-root data/game/normalized \
  --output artifacts/hierarchical-route-candidates/baseline.json
```

The command was run three times with distinct output names. The retained JSON
reports are local artifacts and are not source assets.

## Results

| Run | Complete samples | Searches | Expanded states | Seconds | Samples/min |
| --- | ---: | ---: | ---: | ---: | ---: |
| 1 | 45,000 | 49,500 | 26,002,000 | 8.758 | 308,287 |
| 2 | 45,000 | 49,500 | 26,002,000 | 7.550 | 357,632 |
| 3 | 45,000 | 49,500 | 26,002,000 | 7.452 | 362,312 |

Median throughput was **357,632 samples/min**. The first measured process was
slower than the two warm runs, so later comparisons should retain the same
three-run procedure and use the median.

| Case | Termination | Root candidates | Minimum dissimilarity | Mean dissimilarity | Later survival |
| --- | --- | ---: | ---: | ---: | ---: |
| open single | candidate limit | 6 | 0.125 | 0.745 | not applicable |
| crossed follow-up | candidate limit | 6 | 0.111 | 0.744 | 6/6 |
| shared-capacity follow-up | candidate limit | 6 | 0.071 | 0.615 | 6/6 |
| sealed frontier | frontier exhausted | 0 | not applicable | not applicable | not applicable |

All 45,000 returned samples passed complete native replay. Native rejection was
zero and the measured native-valid ratio was 1.0. Every root candidate in both
follow-up cases admitted and completed the specified later connection using the
anchor follow-up candidate. The sealed case searched all nine compatible port
pairs and returned a fixed-request local proof rather than a global layout
infeasibility claim.

## Decision

The non-learned point-to-point candidate boundary passes its first integrated
gate. It supplies distinct complete macro witnesses, exact revision-atomic
replay, explicit work accounting, downstream survival measurement and useful
CPU-parallel throughput. The next subsystem may add learned opportunity-cost
guidance while keeping this interface, anchor path, panel and work limits fixed.

This result does not admit the full architecture. The current constructive
physical schema still lacks complete shared-network component legalization and
catalog-backed crossings at this boundary, and the panel is too small to
estimate full-factory success.
