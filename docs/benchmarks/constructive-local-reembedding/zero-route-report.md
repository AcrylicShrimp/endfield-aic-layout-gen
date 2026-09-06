# Recognized zero-route repair

## Correctness result

Two new regression tests fail before the fix and pass afterward. The path API now
accepts an explicitly authorized one-cell path after bounds/occupancy checks and
only without directed arms. Reembedding authorizes only extracted schema-2 zero
chains. Arbitrary coincident endpoints still reject. No game or validator rule changes.

The matched first-round control starts from terminal-identity-results.json and repeats
the same 512 fair candidates in both near/far orders. Equal-endpoint failures fall
225 -> 0; remaining no-path failures change 432 -> 462. Best score remains exactly
2,484 / 424 / 40. Removing a false rejection is not evidence of an area improvement.
Detailed matched report: target/zero-route-matched/report.json (1,024 attempts).

The stage-0 upper-bank replay still fails all sixteen rip-up/order/paired-port trials,
now exclusively with no-path. The relaxed body-only route is nineteen steps versus
six before movement. Adding other occupied terminal cells to the relaxed obstacles
makes it unreachable: it is not merely an unfortunate ordinary belt-route order.
The relaxed path crosses other F28 inputs and the F28/F20 shared output/input cell.
This is evidence about the retained port/placement neighborhood, not game infeasibility.

## Complete incumbent

Three fair rounds from fair-coalescence-results.json reach 2,484 / 404 / 38, compared
with 2,484 / 406 / 40. All 59 facilities and seven components validate, at 46x54.
This continuation is not a matched proof that the fix caused those score gains.
Native full and close-up inspection remains required for area work: the selected
full render was inspected and its substantial middle/right dead space remains.

Observed optimization elapsed times: 15.57s matched round, 40.91s continuation.
These are individual batch observations after an app restart, under different load
from earlier reports; do not infer a speedup. The Tailscale viewer had no listener
after the interruption, was restarted, and its page then passed six viewport/theme
checks. No automatic application restart was performed by the research scripts.

## Verification and next gate

- 217 Node tests pass, including exhaustive small-grid A* controls.
- cargo check --workspace and 524 Rust tests pass; two existing ignores, no Rust edits.
- Eight original-graph and sixteen crossing controls pass for the selected witness.
- Reembedding inputs stay immutable and all original flow/port validation gates remain.
- Scope agrees with AGENTS.md and the constructive-planner design: complete-factory
  transactional research only, no constructor or global-exact production cutover.

Next test unused compatible source/target ports jointly for failed physical chains.
The existing paired-port proposal only permutes active equal-commodity slots; it does
not explore the vacant catalog ports. Initial individual F28 input changes alone
still fail, so do not claim the suggested vacant-port repair already works. Use the
shared reembedding engine and validate any full candidate before promotion. The broader
area/recipe/product goal remains open.
