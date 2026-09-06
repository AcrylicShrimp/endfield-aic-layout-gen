# Coordinated neighboring-interface port assignment

## Outcome

The formerly blocked upper-bank move can be restored by assigning the neighboring
interfaces together. It does **not** improve the current factory. The complete best
remains 45x54 / 2,430 area / 411 transport cells / 37 turns from
`vacant-port-results.json`. No production constructor or validator semantics change.

The new diagnostic chooses terminals from the measured obstruction, then optionally
includes their opposite interface ends. On this case that adds the two F43 outputs
feeding the obstructing F28 inputs. All terminal identities and rates remain fixed.
Enumeration of this small injection domain suffices; no Pumpkin dependency or global
placement-routing solver is introduced. The domain ceiling is explicit, so this is
not proposed as an unbounded scalable assignment algorithm.

## Same-parent controls

Both controls replay upper-bank stage 0 against its historical 46x54 parent. They use
the same proposed seven-facility translation, near/far routing with and without the
failed chain prioritized, and at most eight new crossings.

| Scope | Selected terminals | Assignments | Route trials | Raw candidate records | Time |
| --- | ---: | ---: | ---: | ---: | ---: |
| Obstruction cells and failing ends | 4 | 240 | 960 | 0 | 3.91s |
| Also opposite interface ends | 6 | 1,440 | 5,760 | 160 | 21.65s |

Both assignment domains exhaust without a node or candidate limit. The first visits
337 partial nodes, the second 2,497. This exhausts their stated port domains, not
all physical paths, placements or terminal sets. The 160 successful records contain
40 distinct witness geometries; four routing orders often duplicate a witness.
The four-terminal control restores the original failed chain in 253 trials but loses
another route. Its failures are chain 57 (532), 41 (357), 58 (51), and 42 (20).

Frozen reports: `target/cooperative-upper-terminals-frozen/report.json` and
`target/cooperative-upper-interfaces-frozen/report.json`. All recorded engine hashes
match current source. A prior full run reproduces record 1164's witness exactly;
its 22.62s observation differs from the frozen 21.65s run. These are replay batch
costs, not factory generation latency or evidence of a speedup.

### Confirmed joint repair

Representative successful record 1164 retains F42's output port. It changes:

- F28 inputs: original terminals on slots 0, 1, 2 go to slots 1, 2, 0 respectively.
- F43 outputs: original terminals on slots 0, 1 go to slots 1, 2 respectively.

The two F43->F28 routes remain one step each. F42->F28 becomes a **59-step** route
(previously six), using a new bridge at (32,7). This long detour is the cost of opening
the upper approach under these fixed translated bodies and this routing portfolio.
Labels and slots describe this recorded case only, never candidate-generation rules.

With the same parent, motion and near order, retained ports fail on chain 57;
F28-only changes fail on chain 41; F43-only changes fail on chain 42; joint changes
produce the candidate. These are verified replay ablations, not proofs that all
other single-owner assignments or routes are impossible.

## Independent validation and improvement branch

Record 1164 passes full original-graph geometry, recipe coverage and external-margin
validation at 2,484 / 478 / 43, with 59 facilities and nine components. The existing
compaction runner validates the raw candidate before exploring it; 24 compaction
stages over two rounds do not improve it. It is an experimental alternative, not a
replacement for the current best factory.

A private three-round coalescence continuation tests 5,547 requests in 37.44s and
reaches 2,484 / 473 / 43. No candidate fails the independent validation gate, but
there is no area improvement. This branch also loses to the retained global best.
See `target/cooperative-upper-validated-final/report.json` and
`target/cooperative-upper-branch/report.json`. The first attempted compaction command
used nonexistent graph/validator paths and failed before validation; it is excluded
from evidence. The corrected command uses `gap-1-placement.json` and `aic-witness-probe`.

## Visual and regression verification

Personally inspected native full and upper-pocket focused views for the failed
four-terminal assignment, validated joint repair, and final continuation. All three
pages pass six viewport/theme checks, with working zoom and pan. The long pink
upper-right detour and the remaining central voids are visibly poor. The failure
overlay remains explicitly unvalidated and never overwrites the solid parent geometry.

- Repaired branch: `http://100.114.24.86:8765/bound-interconnect-closure-cooperative-branch.html`
- Joint repair: `http://100.114.24.86:8765/bound-interconnect-closure-cooperative-repaired.html`
- Failed control: `http://100.114.24.86:8765/bound-interconnect-closure-cooperative-terminal-failure.html`

227 Node tests pass; cargo check and 524 Rust tests pass (two existing ignores).
Eight original-graph controls and 22 crossing controls pass on the validated branch.
Tests cover exact small injective domains, reserved slots, simultaneous swaps,
projected-body screening, deterministic output, input immutability, and both finite
budgets. Invalid CLI scope returns an explicit error. No Rust files or dependencies
changed. The related terminal-access diagnostic separately passed all 2,187 small-grid
hard/soft obstacle assignments against exhaustive simple paths.

## Next decision

The obstruction-owner diagnostic and one-hop interface expansion are useful, but
brute replaying this portfolio for every relocation would be expensive and often
low-yield. Do not enable it as a blanket production retry. Next combine projected
terminal accessibility and route-length lower bounds to rank coordinated assignments,
and test modest nearby facility relocation to avoid the long upper detour. The failed
fixed-body approach is not evidence against bounded local packing/port oracles.
Keep the 2,430-area incumbent while those alternatives run transactionally.

Compared against AGENTS.md and the constructive-planner design: this remains isolated
improvement research with complete-factory acceptance, area-first scoring, bounded
private trials and explicit diagnostics. There is no global infeasibility claim,
global facility-only constructor, production canvas ceiling, or premature Rust cutover.
Acceptable compactness, broader recipes, cleanup and the web product remain unfinished.
