# Milestone 5: matched-budget findings

Status: complete. The separate staged prototype produces independently checked full factories
for battery, citron powder and both explicitly separate enriched-iron inventories. This closes
the research goal, not the general optimizer or a production migration. See the
[requirement audit](completion-audit.md) for the full scope and evidence.

## Experiment and integrity

The [comparison contract](comparison-contract.md) preceded implementation. The executable
`comparison-plan.json` assigns four inventories × four proposal caps × twelve variants = 192
fresh runs. M3 varies its first family; M4 varies exhaustive/interleaved enumeration,
partial-area/remaining-fit ranking, and one/eight actual routed trials. Original game catalogs,
utilizations, flows, boundaries, proposal sets and all other limits remain unchanged.

At each checkpoint, every variant receives the same allocated vector: 128/256/512/1024
proposals, 200,000 combined route/access expansions, 512 validations, 30,000 construction ms,
and 8,192 fit checks. M3 and Partial consume zero fit checks. Local M4 slices and all replay
trials share that total. Equal allocation does not imply equal consumption or equal CPU cost;
the complete used/unused vectors and elapsed measurements are recorded for every row.

Both `target/chain-m5-comparison-initial/` and `target/chain-m5-comparison-release/` completed
all 192 rows. Their completion, normalized scores, covered inventory, semantic signatures,
allocated/consumed deterministic work, termination, trial counts and witness hashes match.
Timings and output-dependent provenance are deliberately not expected to match.

There are 76 complete rows, each separately accepted by `verify-factory.mjs` with its original
request and the unchanged release Rust probe. All 116 stopped rows remain visible with null
complete scores: 104 global-budget terminations, ten trial limits and two connected-window
limits. These are overlapping deterministic controls, not 192 independent statistical samples
and not an estimated general success probability. Every recorded source hash was rechecked.

Release `report.json` SHA-256:
`3a77413ec5683a70c94642b7319e5fe4ca0d56651693e86d305c8577665f7623`.
It contains every original/derived input, implementation and validator identity, selected
witness/audit identity, decision-report path and summary. Post-run audit work is separately
recorded; it never generates or selects a candidate.

## Quality at the original 1,024-proposal ceiling

Each cell is complete used area / physical transport cells / port-arm-inclusive turns.
`—` means no complete incumbent, not infeasibility. `E/I` means exhaustive/interleaved,
`P/F` means partial-area/remaining-fit ranking, and the suffix is the routed-trial limit.
No column mixes inventories. Human internal references omit external routes and are excluded.

| Single-run variant | Battery 8 | Citron 4 | Iron 9 | Iron 8 |
| --- | --- | --- | --- | --- |
| M3 straight-first | — | 230 / 24 / 4 | 476 / 36 / 6 | 330 / 35 / 6 |
| M3 left-first | — | 224 / 95 / 15 | — | 340 / 61 / 16 |
| M3 right-first | 240 / 33 / 10 | 224 / 98 / 15 | — | 306 / 56 / 14 |
| M3 fold-first | — | 152 / 20 / 9 | 341 / 48 / 20 | 286 / 44 / 18 |
| M4 E/P/1 | 220 / 38 / 12 | 230 / 24 / 4 | — | 312 / 43 / 11 |
| M4 E/P/8 | 220 / 38 / 12 | 228 / 25 / 6 | 364 / 35 / 9 | 312 / 43 / 11 |
| M4 E/F/1 | — | 230 / 24 / 4 | — | — |
| M4 E/F/8 | — | 228 / 25 / 6 | 364 / 35 / 9 | — |
| M4 I/P/1 | 220 / 38 / 12 | 176 / 19 / 6 | 340 / 40 / 15 | — |
| M4 I/P/8 | 220 / 38 / 12 | 162 / 19 / 7 | 340 / 40 / 12 | 280 / 29 / 10 |
| M4 I/F/1 | — | 176 / 19 / 6 | 340 / 40 / 15 | 285 / 37 / 12 |
| M4 I/F/8 | — | 162 / 19 / 7 | 340 / 40 / 12 | 285 / 37 / 12 |

I/P/8 and E/P/8 each finish all four requests with one uniform policy. This is not a hidden
recipe dispatcher. `replay-partial-policy.json` makes I/P/8 reproducible explicitly; the
original `replay-policy.json` remains I/F/8 and still fails battery. I/P/8 is a useful measured
baseline, not the best possible layout: M3 fold-first citron has smaller area (152 versus 162).
Selecting a winner from twelve runs would cost twelve runs; the table is not a free portfolio.

The uniform I/P/8 artifacts have sizes 11×20, 18×9, 17×20 and 14×20, respectively. Their
witness hashes equal the four visually inspected M4 selections in the
[replay report](replay-report.md#reproduction-and-artifacts). The 24 existing browser cases
remain valid for those exact geometries; M5 introduces no renderer changes.

## Completion versus proposal allowance

Counts are out of four fixed requests, including the two distinct iron inventories. Other
resource ceilings are fixed. Per-inventory curves and scores are in `summary.json`; these
aggregate counts do not average layout quality across recipes.

| Variant | 128 | 256 | 512 | 1,024 |
| --- | ---: | ---: | ---: | ---: |
| M3 straight-first | 1 | 3 | 3 | 3 |
| M3 left-first | 1 | 1 | 2 | 2 |
| M3 right-first | 1 | 1 | 2 | 3 |
| M3 fold-first | 1 | 1 | 1 | 3 |
| M4 E/P/1 | 0 | 1 | 2 | 3 |
| M4 E/P/8 | 0 | 1 | 2 | 4 |
| M4 E/F/1 | 0 | 1 | 1 | 1 |
| M4 E/F/8 | 0 | 1 | 1 | 2 |
| M4 I/P/1 | 1 | 1 | 1 | 3 |
| M4 I/P/8 | 1 | 1 | 1 | 4 |
| M4 I/F/1 | 1 | 1 | 2 | 3 |
| M4 I/F/8 | 1 | 1 | 2 | 3 |

M3 straight-first already finishes three requests at 256 proposals; I/P/8 still finishes only
citron at 512. Shortlisting and continuation have an opportunity cost before the first
complete result. M4 does not dominate M3 under small budgets.

## Controlled findings and failure classification

Holding M4 enumeration and ranking fixed isolates additional routed trials. At cap 1,024,
E/P/1 iron-nine stops with five facilities at G1 (333 proposals / 27,662 expansions / 16
validations); E/P/8 completes at 364/35/9 using 710/39,400/56. I/P/1 iron-eight stops with
six facilities at G2 (370/15,463/25); I/P/8 completes at 280/29/10 using 700/19,392/64.
These are actual frontier repair results, not body-fit estimates or merely altered reporting.

Additional trials also improve complete incumbents: I/P citron 176/19/6 → 162/19/7 and
iron-nine 340/40/15 → 340/40/12. Area correctly outranks turns in the former; the turn
tie-break applies only after area and cells tie in the latter. The M4 iron-eight trace also
reverses internal areas 230/240 into complete areas 288/280. Tests inspect that actual routed
continuation and exact preservation of the unrelated plant cycle and first iron chain.

More trials are not invariably useful. E/P battery performs a later failed boundary attempt
and retains the same incumbent; E/P iron-eight spends additional work without improvement.
I/P battery already exhausts its connected window, so allowing eight trials changes nothing.

Holding enumeration and trial count fixed isolates body-fit ranking. It adds work without
changing citron or iron-nine results at 1,024; it loses battery completion and worsens I/P
iron-eight from 280/29/10 to 285/37/12. However, I/F iron-eight completes at cap 512 while I/P
does not. Thus Fit is not always worse, but these controls do not support a general benefit.

The battery Fit failure is evaluation plus bounded ownership, not a missing bend primitive:
Fit prefers straight F1 by estimated remaining-body area, later stops at G3, and cannot reach
that earlier choice through its connected recent suffix. The frozen consumer separates the
last fan-in branches. A fragmented-hole test proves that Fit sees body shape, but it cannot
represent simultaneous future port corridors and external access. Window exhaustion is not
global infeasibility and does not authorize secretly expanding the neighborhood.

Enumeration also matters independently: at fixed P/8, interleaving yields better citron and
iron scores than exhaustive order. M3's fixed-family results show that straight, bends and
folds each have useful cases. The M3-to-M4 comparison changes several mechanisms, so it cannot
assign all gains to completion lookahead alone.

## Research conclusion

The prototype reproduces flexible connected chain growth, spatial port choices, mixed local
approaches, seed-cycle closure, multi-input consumer assembly and bounded reconstruction with
real routed completion. It does not reproduce the human examples' compactness, nor is it a
general optimizer. Long return routes, inaccessible future corridors and greedy candidate
omissions remain visible. The evidence generalizes only across the measured component-tree
graphs with singleton or reciprocal two-node components and explicit perimeter boundaries.

Keep this as an experimental baseline. Retain actual continuation and complete-incumbent
comparison; do not promote greedy body-fit ranking to production. A next controlled research
slice should compare earlier limited continuation against an explicitly defined fan-in window
that may retain connecting anchors while replaying adjacent suppliers. Specify frozen ports,
route ownership and one shared budget before implementation, then start with the existing
battery failure. This is a proposed experiment, not an implemented escape hatch or permission
to widen the current contract.

Larger SCCs, reconvergent component DAGs, shared physical trunks/junctions, mixed-transport
full factories, warehouse-specific hookups, power and cycle bootstrap remain unsupported or
outside this study. Single-kind pipe chain tests do not establish mixed full-factory support.
No global proof, CP oracle, new propagator or production replacement was introduced.

## Reproduction and verification

Run from the repository root; use a fresh output directory:

```sh
cargo build --release -p aic-cli --bin aic-witness-probe
node docs/benchmarks/chain-prototype/run-comparison.mjs \
  --plan docs/benchmarks/chain-prototype/comparison-plan.json \
  --policy docs/benchmarks/chain-prototype/replay-policy.json \
  --recipes data/game/normalized/recipes.json \
  --facilities data/game/normalized/facilities.json \
  --items data/game/normalized/items.json \
  --transports data/game/normalized/transports.json \
  --components data/game/normalized/logistics-components.json \
  --validator target/release/aic-witness-probe \
  --output-directory target/chain-m5-comparison-reproduction
```

For one uniform-policy factory, substitute `run-replay.mjs`, replace `--plan` with
`--request docs/benchmarks/chain-prototype/battery-factory.json`, and set `--policy` to
`docs/benchmarks/chain-prototype/replay-partial-policy.json`; keep all catalog/validator flags
and choose another fresh output directory. Substitute `citron`, `iron-nine` or `iron-eight`
for the other original requests. The standalone verifier command is documented in the
[factory report](factory-report.md#reproduction-and-artifacts).

Artifacts are under `<output>/<case>/<cap>/<variant>/`: immutable derived request/policy,
`run/report.json`, all prefix validations and decisions, optional complete witness/page,
independent `audit/report.json`, and `comparison-row.json`. Root `report.json` and
`summary.json` include failures. Exit 0 means a completed comparison, not that every row
constructed a factory; unexpected process, budget, hash or audit failures exit 1 visibly.

Nine comparison tests cover the 192-run plan, exact semantic preservation, policy identity,
equal allocations and direct work accounting, common turns, separate inventories, stopped
curves, original-request audit acceptance and immutable CLI output. The full command
`node --test docs/benchmarks/constructive-*/*.test.mjs docs/benchmarks/user-layout-reconstruction/*.test.mjs docs/benchmarks/chain-prototype/*.test.mjs`
passes 355 tests with no skips. `cargo check --workspace` passes;
`cargo test --workspace --quiet` passes 524 tests with two existing ignored tests.
No Rust files or dependencies changed, so Rust formatting is not applicable to this slice.
