# Terminal-access diagnosis

## Measured result

The upper-bank stage-0 failure in `target/bank-fair-coalescence/report.json` needs
at least **two occupied facility-terminal cells** on any path in the declared finite
relaxation. Its lexicographic optimum has 49 steps and contacts F28 input-belt-0 at
(19,1) and input-belt-1 at (19,2), both carrying item-plant-moss-powder-2. The failing
chain 57 carries item-plant-moss-powder-3 from F42 to F28. This changes the next
neighborhood: include the two other input owners as well as the failing endpoints.

The earlier 19-step body-only detour crosses a third occupied terminal cell on the
right. Minimizing blocker cells first deliberately chooses a longer detour. This
does not measure the cost or feasibility of clearing those cells and restoring all
displaced routes. Ordinary wires, components and arm directions are ignored. There
is no factory-infeasibility claim, and no relaxed path enters a witness.

The retained/priority/vacant replay still rejects 24 trials. Its complete report is
`target/terminal-access-upper/report.json`, including paths, owner identities, bounds,
source hashes and input provenance. The original validated parent is unchanged. This
diagnostic replays an older 46x54 parent, not the current 45x54 best factory in
`vacant-port-results.json`; no new area improvement is claimed.

## Verification and visual review

All 2,187 hard/soft/free 3x3 assignments match independently enumerated simple paths
on blocker count and steps. Tests also cover shared owners, hard/soft overlap,
endpoint exemptions, budget unknown, malformed bounds, deterministic tie handling,
owner-copy isolation and layer-specific translated obstacles. 224 Node tests and
524 Rust tests pass (two existing ignores); cargo check passes. No Rust changes or
dependencies. Logs use the `target/terminal-access-` prefix.

The native full and F42/F28/F43/F54 focused overlay were personally inspected. Two
purple obstruction squares mark the newly identified F28 terminals; the purple
dashed detour is explicitly labeled NOT a valid route. Red proposed bodies and the
solid unchanged validated factory remain distinguishable. Six viewport/theme checks
pass. Viewer: `http://100.114.24.86:8765/bound-interconnect-closure-terminal-access.html`.

Compared with AGENTS.md and the constructive-planner design, this is a read-only
diagnostic of a transactional improvement failure, not a construction cutover,
global exact model, production canvas limit, or port/route semantic relaxation.
The next experiment must preserve terminal identities/rates and independently
validate any coordinated port candidate before accepting it.
