# Pipe-supply north placement

## Result and scope

The user's requested north preference is implemented in the isolated placement-only diagnostic.
All 59 Heavy Xiranite facilities remain placed with legal rotations, nonoverlapping rectangles and
one-cell clearance. There are no assigned ports or physical routes, and factory validation is false.
No production constructor, routing order, runtime catalog or exact solver changes.

The former F03 liquid-only reactor becomes **F01**, north of the final consumer F00. Its upstream
reactor becomes F04. The two direct belt suppliers become F02/F03 in the same/below rows. F labels
change with discovery order; `comparison.json` maps every instance across runs by stable ID.

| Metric | Corrected baseline | Pipe-supply north |
| --- | ---: | ---: |
| Facilities | 59 | 59 |
| Bounds | 52 x 92 | 52 x 98 |
| Bounding area | 4,784 | 5,096 (+6.52%) |
| North-input pipe relations with supplier fully north | 0 / 1 | 1 / 1 |
| Internal pipe requirements | 2 | 2 |
| Logical belt center Manhattan span | 1,070 | 1,004 (-6.17%) |
| Logical pipe center Manhattan span | 18 | 18 |
| Non-forward logical requirements | 18 | 18 |

This is a directional-placement improvement, not an overall score improvement: bounding area is
worse, and area is the production score's first priority. Logical center spans are not routed
lengths. The remaining non-forward requirements, density near unused approaches and the west-input
reactor exception still need physical routing tests. No inference of routability follows from the
single satisfied north relation. Keep both frozen candidates for later comparison.

## Policy

1. Keep active belt-output/input alignment before active pipe alignment when selecting rotation.
2. Compute each node's upstream closure including itself; count distinct internal outgoing pipe
   requirements from that closure. This is an unweighted graph-edge count, not physical port count,
   flow rate or predicted route count. Shared edges count once per closure; cycles terminate.
   External supply/surplus does not rank internal branches and remains explicit in the output.
3. Discover adjacent facilities by decreasing branch pipe count, then belt-first, upstream-first,
   stable-edge ties. This replaces the old always-belt-first neighbor sort.
4. Keep the existing upstream/downstream x position and top-edge row anchor. Consider collision-free
   obstacle-edge rows plus rows immediately above/below already-placed north-input pipe neighbors.
   Rank candidates by north pipe relation violations, then branch-priority vertical inversions among
   horizontally overlapping clearance columns, then top-edge distance, then north tie-break.
   Higher pipe-count branches prefer northern rows in those columns. This is not a global latitude
   ordering of every disconnected facility, nor an atomic subtree packing guarantee.
5. Never move prior facilities. North relations and branch order are soft preferences; cycles may
   conflict. Final unmet north relations produce a stable diagnostic, not infeasibility.

The actual rotated catalog decides whether a pipe consumer has a north input. In this graph F00
does; the liquid-only reactor F01 has west inputs. That second connection stays a horizontal
relationship rather than being forced north. Port choice and approach accessibility remain untested.

Every placement records `branch_pipe_outputs`; each decision records the related pipe edges and
selected row's scores. `pipe_relations` records final consumer faces and north relation outcomes.
The viewer keeps ports visible by default and exposes branch/row scores in the facility inspector.
Production policy/design were reviewed: this remains the explicitly approved standalone geometry
exception, not a change to connected transactional production construction or pipe-first routing.

## Reproduction and timings

```sh
node docs/benchmarks/constructive-placement-only/time.mjs NEW_DIRECTORY 20
node docs/benchmarks/constructive-placement-only/compare.mjs artifacts/benchmarks/constructive-corrected-ports/heavy.json NEW_DIRECTORY/00-placement.json NEW_COMPARISON_JSON
```

Twenty sequential fresh-process repetitions with release graph CLI and Node v26.5.0 on darwin/arm64;
warm filesystem, no concurrent build or test load, first sample retained. Same scope as the corrected
baseline benchmark: graph generation through placement report and HTML writes, without fsync or
browser loading. All normalized geometry reports are identical after removing timing/provenance.

- Placement median **25.66 ms**, P95 25.97 ms, range 25.06–26.52 ms.
- Whole graph + placement + JSON/HTML pipeline median **204.41 ms**, P95 209.04 ms,
  range 197.24–209.06 ms. No routing included.
- `timing.json` retains raw samples, phase measurements and binary/input hashes. Full temporary
  artifacts: `/tmp/aic-pipe-north.83Sq17/`. `heavy.json` is the frozen current placement;
  `comparison.json` preserves stable-ID comparisons against the previous frozen baseline.

## Verification

- Eleven Node tests pass: straight belt chains, branch sorting and row order, upstream/shared/cyclic
  counting, actual north/west faces, soft cyclic conflicts, geometry, orientation, input validation,
  safe HTML embedding and compact timing CLI output/collision checks.
- `cargo check --workspace`, `cargo test --workspace` pass: 509 tests, two existing research ignores.
- Manual Heavy Xiranite and two-facility seed-cycle CLI runs pass; comparison validates unchanged
  instance sets and logical internal/external requirements.
- Six browser configurations (1400/736/360 px, light/dark) pass: stage-five fit, selected branch
  scores, default ports, logical overlay, north relation and no horizontal page overflow. Inspected
  the first-five-facility screenshot showing F01/F04 above the belt suppliers.
- Published `http://100.114.24.86:8765/placement-only-pipe-north.html` through the existing tailnet
  server. `placement-only-corrected.html` remains unchanged for comparison.

Next: review the directional placement, then mechanically route frozen geometry and record actual
port/corridor failures. Do not expand this result into a routing-success or global-optimality claim.
