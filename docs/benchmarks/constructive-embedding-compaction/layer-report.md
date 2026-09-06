# Retained-order diagnosis and independent transport layers

## Findings

The central void is not established as necessary. The old compactor deliberately kept
both row order and incidental belt/pipe coincidences. It keyed geometric points only by
XY, so a pipe and a belt at the same coordinate could join otherwise unrelated rigid
coordinate groups. This was a conservative improvement restriction, not invalid factory
geometry or a claim that the game couples the two transport layers.

The diagnostic reconstructs the actual compaction certificate's facility partitions
exactly, on both axes. An equality-only ablation removing incidental cross-layer ties
splits the largest X group from **19 facilities into 9/7/2/1**. In Y, the seven-member
F37/F48/F26/F19/F13/F08/F03 group separates into the F37/F48 pair and the five-member
process group. Facility bodies and terminal attachment ties remain.

For eight automatically selected interior empty rectangles, nine of sixteen axis
queries have a tight relative-separation certificate. Example: the 7x11 rectangle at
(23,33) has a Y boundary separation of twelve. An actual three-inequality path requires
all twelve under the retained model: F16/F10/F05 group -> F00 -> route-only group ->
F26/F37/F48/F19/F13/F08/F03 group. This proves a restriction of that embedding, not a
game-required twelve-cell gap. The larger 10x9 rectangle is not fully locked by its
strongest boundary pair: X actual/required 11/1, Y 10/7. Pair slack does not prove that
the corresponding whole groups can move or that a complete factory can be rerouted.

Native renders show the remaining structural issue: unequal-height source banks on the
left and differently positioned process rows on the right leave a large central pocket.
Removing one artificial coupling does not reselect their relative order or internal
shape. Filling a pocket alone is not the objective.

## Implementation and controlled result

`order.mjs` traces actual difference-constraint paths with explicit work budgets. Tight
paths are reconstructed lazily from BFS predecessors, including zero-weight cycles.
`order-probe.mjs` emits JSON certificates, rigid-group ownership and boundary traces.
It does not produce or promote hypothetical layouts. The ablation is checked against
the actual compactor before its results are interpreted.

`compactEmbedding` now has a measured `layerPolicy` experiment. `coupled` retains the
historical control. `independent` uses typed layer coordinates and per-layer row order;
every body is represented on both layers and tied to its one facility pose. Ports,
components, same-layer coincidences and directed chain topology remain constrained.
Both compaction and local-reembedding CLIs expose `--layer-policy`; detailed output
stays in JSON. No global solver or production Rust API changed.

All rows start from `constructive-local-reembedding/crossing-results.json`,
47x58 = 2,726 / 454 cells / 40 turns.

| Experiment | Coupled | Independent |
| --- | --- | --- |
| Fixed geometry, XY, 16-state/8-round plateau | 2,726 / 454 / 40 | 2,726 / 451 / 40 |
| Fixed geometry, YX, same budget | 2,726 / 454 / 40 | 2,726 / 451 / 40 |
| Joint moves/routes, 256 positions, three rounds | 2,726 / 450 / 41 | **2,679 / 444 / 45** |

Both joint runs execute 1,758 attempts. Batch elapsed: 67.29s coupled, 72.13s independent;
these run concurrently and are not a latency benchmark. No compaction diagnostics or
geometry validation rejections occur. The first area improvement translates the
F52/F57/F53/F42 bank by (+1,0); raw geometry remains 2,726, then independent-layer
compaction lowers the height by one. The gain is in the upper factory, not elimination
of the central void. A follow-on independent batch makes 586 attempts in 19.70s and
retains the same witness at `unimproved-neighborhood`.

The selected factory is **47x57 = 2,679 area**, 444 physical transport cells, 45 turns,
59 facilities and seven components. Compared with the bridge milestone this is 1.72%
less area. Eight original-graph controls pass; sixteen unchanged/deleted/missing-owner
bridge controls pass. The selected witness hash matches the original-graph audit.

## Visual result and next decision

Personally inspected native full and central close-up renders. Six viewport/theme
checks and pan/zoom/Fit pass. The long cyan return around the right/bottom still exists.
The largest interior empty rectangle grows from 90 to **99** despite lower whole area;
the largest exterior empty strip grows to 152. Do not claim that the void is solved,
or optimize fill ratio in place of area.

The current evidence supports keeping independent-layer compaction as an explicit
research option. The next gate is coordinated relative-order/shape changes of the
surrounding groups with fresh boundary routing, rather than another pure coordinate
contraction pass. Pumpkin is the user's optional oracle idea, not a mandated immediate
switch. Evaluate it against constructive joint-move alternatives. The full goal remains
open: additional recipes, cleanup and web product work are not yet complete.

## Reproduction and verification

Diagnostic:

```text
node docs/benchmarks/constructive-embedding-compaction/order-probe.mjs
  --baseline docs/benchmarks/constructive-local-reembedding/crossing-results.json
  --catalog-directory data/game/normalized --output-directory NEW_PATH
```

Fixed comparison: compaction `run.mjs`, original graph/catalog/validator flags,
`--rounds 8 --axis-order xy|yx --state-budget 16 --layer-policy coupled|independent`.
Joint comparison: local-reembedding `run.mjs`, same inputs,
`--rounds 3 --mode joint --move-limit 256 --move-family cone --endpoint-policy
terminal-flexible --junction-policy downstream --max-new-crossings 4 --port-policy
retained --layer-policy coupled|independent`. Follow-on uses the independent report
as baseline. Always use fresh output directories.

- 193 Node tests pass, including 200 exhaustive relative-bound graph controls,
  certified negative/zero-cycle paths, equality ablation and invalid budgets.
- Every runtime facility/rotation/port is checked on both axes and both layer policies.
  Synthetic controls distinguish legal cross-layer coincidence from forbidden same-layer
  overlap and ensure bodies obstruct both layers. Input immutability is checked.
- `cargo check --workspace` and `cargo test --workspace` pass: 524 tests, two existing
  ignores. No Rust files changed. Complete CLI runs independently validate all promoted
  witnesses; separate audits cover original graph requirements and bridge integrity.
- Policy review: complete-witness transactional improvement, runtime catalog data,
  no production canvas restriction or global exact fallback. The production frontier
  contract in the constructive planner design remains unchanged.

Machine-readable evidence and selected artifact: `layer-results.json`.
