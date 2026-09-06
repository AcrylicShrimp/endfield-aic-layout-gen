# Bounded cooperative terminal-port replay

## Contract

This explicit diagnostic takes a terminal-access replay report, runtime catalogs,
and a new output directory. `--scope terminals` (default) uses the first failed retained-port chain and the
owners on its minimum-contact path to select up to eight facility terminals. The
selected set includes singleton facility ends of the failed chain. No IDs, recipes,
labels, dimensions or port counts are hardcoded. Unsupported shared ends remain
fixed; the normal assignment gate still rejects ineligible changes. `--scope interfaces`
also includes singleton facility terminals at the opposite ends of chains incident
to the initial selected set (one hop, not a recursive closure). The eight-terminal
ceiling rejects an oversized group explicitly, without silently dropping owners.

`cooperativePortCandidates` takes a witness, catalogs, movement primitives, selected
unique terminal IDs, a candidate limit (1..4096), and a search-node budget (1..65536).
It enumerates injective compatible assignments to runtime ports, including selected
terminals' currently occupied slots and vacant slots. Unselected occupied slots stay
reserved. New connection cells inside projected bodies are screened. Terminal item,
transport, identity, direction and rate do not change; source/target margins are not
reallocated. Cross-terminal and component occupancy remains the existing assignment
gate's responsibility. No geometric collision constraint is silently weakened.

The recursive search charges every visited partial assignment to its node budget.
Outputs contain candidates (`portAssignments`), selected IDs and per-terminal domain
sizes, node count, limits, and termination `exhausted`, `node-budget`, or
`candidate-limit`. Truncation is not local infeasibility. Even exhaustive enumeration
is only this fixed-terminal-set port domain, not all assignments or placements.

Each candidate is replayed independently from the same parent with near/far orders,
with and without prioritizing the failed chain. The request retains movement,
endpoint/junction policies and crossing budget. Up to 16384 route attempts are allowed
by the fixed four-order portfolio and 4096-candidate ceiling. Existing per-route
expansion bounds remain unchanged; all elapsed costs and rejections are recorded.
Output candidates remain `factory_validated: false` until separately checked by the
original-graph factory validator. Nothing is automatically selected or committed.

Invalid CLI/report input returns `cooperative-probe-failed`. Candidate input errors
use `local-cooperative-ports`, severity info, stage local-ports, with entity references.
Each route failure retains the existing structured diagnostics. Report schema 1
records source hashes, input hashes, request, attempts, domain termination and any
private candidate paths. A diagnostic-source hash mismatch must be explicitly
reported rather than silently claiming exact historical replay.

The native overlay selects the rejected record with the most successfully restored
jobs (first tie). It shows proposed bodies, the failed chain's reassigned endpoints,
the new port list, and a recomputed terminal-access relaxation. Solid geometry remains
the validated parent; neither partial successful jobs nor the relaxed path become
accepted wiring. The representative record index and access result are report fields.

## Verification

Compare a small injection domain to an independently enumerated assignment set;
cover reserved slots, simultaneous swaps, runtime geometry, node/candidate bounds,
invalid input and immutability. Run the measured upper-bank neighborhood. Inspect
each distinct result selected for discussion in the native renderer. No production
cutover or feasible-factory claim follows from port enumeration alone.
