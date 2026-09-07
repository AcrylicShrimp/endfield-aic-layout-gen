# Catalog-backed bridges and simultaneous boundary port swaps

## Outcome

The complete factory improves from 47x61 = 2,867 / 410 cells / 34 turns to
**47x58 = 2,726 / 454 cells / 40 turns**. Area decreases 4.92%; wiring increases.
Area remains the first objective. All 59 facilities remain, with seven components.
Eight original-graph controls pass for the selected witness. Sixteen independent
bridge controls pass: unchanged geometry, plus removing each of five bridges or
either of its two commodity references. These checks do not establish new game rules.

`crossing-results.json` records the new incumbent. The previous factory remains a
valid control. No production Rust cutover or global optimality claim is made.

## Controlled evidence

Common first-batch baseline: `cone-results.json`. Three rounds, joint mode, 128
move positions, cones, downstream junction ownership and terminal-flexible endpoints.

| Variant | Area / cells / turns | Attempts | Batch seconds |
| --- | --- | --- | --- |
| No new bridges, retained ports | 2,867 / 409 / 35 | 656 | 16.41 |
| Up to four new bridges, retained ports | 2,773 / 459 / 36 | 988 | 40.91 |
| Up to four new bridges, paired ports | 2,773 / 459 / 36 | 2,226 | 66.41 |
| Follow-on retained ports, starting at 2,773 | 2,726 / 454 / 40 | 990 | 34.16 |
| Follow-on paired ports, same 2,773 start | 2,726 / 454 / 40 | 2,200 | 56.20 |

Both final-source pairs select byte-identical witness files. No accepted-geometry
validation rejections occur in these four bridge-enabled batches. Timings are concurrent
batch observations, not a controlled latency benchmark. The zero-bridge control predates
the final optional port/unit-bridge changes; ordinary no-bridge routing is unchanged.
Both follow-on portfolios terminate at `unimproved-neighborhood`, not global optimality.

Replay stage 74 of `target/cone-current-source/report.json`: moving F37/F48/F36/F25
into the central void still fails with four or eight new bridges and retained ports.
Swapping an equal-rate pair of boundary ports permits the move with four new bridges.
The final generic replay reproduces the earlier raw witness exactly. Normal refinement
and eight original-graph controls validate that alternative at **2,820 / 494 / 45**.
It fills the pocket but has long nested output/return paths and loses on whole area.
Paired ports therefore prove an additional feasible move, not a better final score.

## Implementation and boundaries

- Runtime catalog poses determine legal one-cell straight perpendicular crossing sites.
  No terminal, body, existing component, bend or reserved arm may be used as a new site.
  Same-item sites are excluded from this proposal family, not declared illegal generally.
- A* tracks bridge budget in its state and traverses each bridge straight. Consecutive
  bridges require no artificial spacer. Only selected poses are installed in a private
  candidate and referenced by both networks. Carrier geometry remains intact.
- Explicit terminal assignments preserve identity, item, owner and exact rate, validate
  simultaneous slot ownership and reroute every affected chain. Adjacent equal-rate
  same-item boundary swaps provide a bounded automatic proposal family.
- Defaults retain old ports and allow zero new bridges. CLI flags are explicit and
  detailed decisions are JSON outputs; stdout is only a completion summary.
- Rejected partial candidates never replace the incumbent. Every selected full geometry
  is independently validated. The original graph audit separately checks production
  coverage and external requirements.

Reproduce the bridge batches using `run.mjs` and the usual graph/catalog/validator flags:

```text
--baseline artifacts/benchmarks/constructive-local-reembedding/cone-results.json
--rounds 3 --mode joint --move-limit 128 --move-family cone
--endpoint-policy terminal-flexible --junction-policy downstream
--max-new-crossings 4 --port-policy retained --output-directory NEW_PATH
```

For the second batch use the first batch report as baseline. Substitute `paired` for
the port control. Generic obstruction replay: `priority-probe.mjs --report
target/cone-current-source/report.json --stage 74 --catalog-directory
data/game/normalized --max-new-crossings 4 --port-policy paired
--output-directory NEW_PATH`. Replay artifacts alone are not validated incumbents.

## Verification and visual interpretation

All 188 Node tests under `docs/benchmarks/constructive-*` pass. The earlier selected
suite passed 147; the count difference is test selection, not 41 newly added tests.
`cargo check --workspace` and `cargo test --workspace` pass: 524 tests, two existing
ignores. No Rust files changed. Bridge negative controls use `crossing-audit.mjs` with
`--baseline`, `--catalog-directory`, `--validator` and `--output-directory`.

Personally inspected actual full and close-up renders, including the validated
port-swapped alternative. Six viewport/theme checks and pan/zoom/Fit pass. The current
factory still has a conspicuous central void and a long cyan return around the right
and lower boundaries. The largest empty rectangle is now an exterior 3x38 strip;
an interior 10x9 rectangle remains. Neither fill ratio nor component count replaces
used area as the objective.

The user clarified that the next question is how these voids arise, not how to fill
them. Existing compaction retains row order, perpendicular segments and geometry
coincidences. Bounded rigid translations do not jointly rearrange the surrounding
groups. Next extract concrete order-lock evidence and test simultaneous relative-order
selection for a bounded group neighborhood. **Pumpkin**, not OR-Tools, is the user's
suggested local oracle, not an instruction to switch immediately. Compare that option
against the measured limitations of the current experiments first. Preserve
internal validated structures, rebuild boundary connections transactionally, and judge
the complete routed footprint. A packing-only solution is not a factory witness.

Policy review: this is isolated complete-layout improvement research with runtime
catalogs, explicit local bounds and independent acceptance. Production immediate-routing
frontier semantics in the constructive planner design remain unchanged. Recipe breadth,
cleanup and web product milestones remain open.
