# Reproduce a rejected large facility move

## Status

Complete. Exact failure reproduced and a valid same-pose counterexample found.
Normal executor, reward and training configuration remain unchanged.

## Contract and completion criteria

Input: immutable request, validated state before evaluation 208 / factory 0 /
action 11, and its recorded action descriptor. Preserve the original one-hour
runtime and checkpoints. The move places facility index 13 at (32, 3).

Output: exact baseline reproduction, failing chain and obstacle evidence,
controlled one-factor probes, and full native validation of any recovered
candidate against the original request. Keep raw captures and candidate witnesses
under ignored artifacts. Record the distinction between a path-only relaxation
and a complete valid factory. A local failure is not global infeasibility.

Use an isolated diagnostic checkout for instrumentation and experimental
neighborhood changes. Do not change normal execution or resume training.

## Activity

- Replayed the recorded action with the normal native extension. It fails on
  chain 28 with `local-no-path`, preserving the exact pre-action state identity.
  Work budgets 5,000, 50,000 and 200,000 all stop after 1,253 total expansions.
  This case is not a simple route-work exhaustion.
- Ran 80 isolated diagnostic cases, including bridge budgets, routing order,
  ports, neighboring reroute sets and invalid out-of-boundary detour controls.
- The original 5,000-work budget succeeds with 12 new bridges and chain 43 first:
  4,964 expansions, full validation, score `[1672,456,108]`. This proves a legal
  transition is blocked by the current executor; it does not prove improvement.
- Audited the actual 2,216-action domain. It exposes singleton and rigid pair
  moves, but no three-plus-facility pose in this state and no combined pose/port/
  neighboring-reroute action. Recorded implications for the user's contract
  coverage question in `docs/reports/2026-09-10.13-local-route-action-contract.md`.
- Preserved a standalone isolated-checkout reproduction tool and compact hashes.
