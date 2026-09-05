# Last-port-preserving path retries

## Result and retained best

The same frozen proposal connects **63/89 internal requirements**, versus 64/89 for the previous
multi-lane result. F31 -> F22 now connects, but other decisions change downstream coverage. This
is a useful local mechanism, **not an overall improvement**. Preserve the prior 64-edge artifact
at `constructive-multilane-routing/routing.json` as the best coverage witness so far.

Three release probe times are 4,449.37 / 4,478.55 / 4,309.78 ms, median **4,449.37 ms**. There are
601 A* calls, 206 last-port path rejections before materialization, 16 post-hoc future-domain
rejections and zero invalid-witness rejections. Summed network cells are 425. Both pipe requirements
connect; 26 internal and seven external requirements remain. No facility moves or rate changes.

## Mechanism

For pending same-layer domains on other facilities containing only unused port alternatives,
record candidate connection cells. If a proposed path consumes every still-free candidate of
one domain, branch on avoiding one of those cells and retry the same attachment pair. Variants
are breadth-first and deduplicated, with at most eight variants per attachment pair and the same
shared 64-A* requirement budget. Do not reserve all unused ports or three-cell escape corridors.

Each physical search records `preservation_conflict` and `avoided_cells`. The report adds
`per_pair_path_variant_limit`; these are additive v3 diagnostics. The viewer identifies path-level
preservation rejections separately from completed-candidate future-domain rejections.

Current endpoint facilities and existing-network domains still use full post-hoc validation,
because spare capacity and shared attachments cannot be represented by this simple cell guard.
Nonempty local domains do not prove end-to-end reachability. This is a bounded constructive
heuristic, not an infeasibility proof or exhaustive alternative-path enumerator. Production's
assigned-port mode keeps its original behavior; all complete bundles independently validate.

## Verification

- Nine focused probe tests pass. The former last-external-port rejection fixture now connects
  via a preserving detour. A two-candidate future-port fixture verifies that one may be covered
  while the other remains free; reserving both would block the current selected endpoint.
  A one-call budget still leaves no partially committed route. Multi-lane rollback tests remain.
- `cargo fmt --all -- --check`, workspace check and full tests pass (518 Rust tests, two existing
  ignores); fourteen Node tests pass. An initially overconstrained test assertion was corrected
  to use a single current target port and two future alternatives, then rerun successfully.
- Three fresh release runs, with no build/test overlap, produce identical final witnesses. All
  59 relative coordinates/rotations, accepted lane-rate sums, trace counts and budgets are audited.
- Browser checks pass at 1400/736/360 px in both themes. Actual overview/multi-lane/failure images
  are saved under `target/shared-results/port-preserving-routing*.png` and inspected.

## Next obstruction and controls

The first failure becomes **F32 -> F23**, placement index 32. Fifteen attachment searches exhaust
the available pairs without reaching the completed-candidate guard. There are no path-level or
post-hoc preservation rejections for this requirement. Increasing resources does not resolve it:

| Padding | A* limit per requirement | Connected | First failure searches |
| ---: | ---: | ---: | ---: |
| 16 | 64 | 63/89 | 15 |
| 16 | 256 | 63/89 | 15 |
| 32 | 64 | 63/89 | 15 |
| 32 | 256 | 63/89 | 15 |

All cases first fail at F32 -> F23. `controls.json` records command parameters, output locations
and timings. These controls rule out those tested resource increments as a remedy; they do not
prove global infeasibility. A free port cell can remain inside a disconnected routing pocket.

Next experiment: a bounded two-connection reroute, preserving facility geometry and the accepted
prefix. Plan an uncommitted route for the failed connection as a temporary reservation, rebuild
the immediately preceding connection around it, then actually route the failed connection in the
original order. Commit only if both requirements and the older covered witness validate.

## Artifacts and scope

`routing.json`, `summary.json`, `timing.json` and `controls.json` preserve this experiment.
Use the same CLI as `constructive-multilane-routing/report.md`; add `--search-padding` and
`--max-astar-per-connection` for the explicit controls. All output paths must be new.
The page is `http://100.114.24.86:8765/port-preserving-routing.html`.

This is still the user-approved frozen-placement diagnostic, compared against AGENTS and the
accepted constructive-planner design. No production cutover, global solver, placement fallback,
catalog change or complete-factory claim is introduced.
