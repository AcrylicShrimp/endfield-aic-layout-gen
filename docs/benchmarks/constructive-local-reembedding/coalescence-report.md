# Link-aligned coalescence: no area gain; coincident-terminal lock exposed

## Outcome and decision

The large empty region remains unresolved. The complete factory retains **47x57 =
2,679 area / 438 transport cells**. A standalone bridge-release validation control
saves one turn, 39 -> 38, and is independently accepted as the new tie-break incumbent.
This is incidental, not the purpose or success criterion of the area experiment.

The new link-aligned grouping family does not shrink the factory. It exposes a more
specific next gate: a facility pair currently sharing one terminal cell cannot separate
under the movement representation. This is not a game-imposed rigid group. Until the
interface can turn that zero-length connection into an ordinary routed connection,
packing is forced to transport unrelated geometric decisions together.

## Experiment

The actual lower seed network links F48 to F36 with a long outer return. F37 and F48
are together at y=37 while F36 is at y=51. The graph/cell-derived proposal family tries
to restore nearby placement without hardcoded labels or recipe rules: choose two
disjoint terminal-closed groups, align connected terminal positions one unit apart,
then translate the combined occupied shape against existing body contacts. Preserve
each group's interior pose, not its old relative ordering or absolute origin.

The initial global bridge-release control produces 77 positions / 154 near/far
restoration attempts; all fail, as does the retained-bridge control. Broadly releasing
all bridges unnecessarily rebuilds unrelated interfaces. The final family releases
only bridges on owner paths incident to either selected group and widens each
pair/direction shortlist from eight to 32 valid positions.

Current-source results:

| Bridge policy | Positions | Repair attempts | Result | Batch time |
| --- | ---: | ---: | --- | ---: |
| Retain old components | 158 | 316 | No candidate restored | 5.56s |
| Release incident crossing anchors | 158 | 316 | No candidate restored | 6.70s |

Both controls use the same proposed poses, 88 terminal-closed groups, an eight-new-
bridge repair budget, terminal-flexible endpoints and independent-layer compaction.
There are 429 available pair/link combinations, but the 250,000-pose ceiling is
reached after starting only 26. This is deliberately not reported as exhaustive pair
search. Of 761 screened body-legal poses, 97 fail retained-terminal collision, 188
endpoint/body collision and 292 fixed-anchor collision; other poses may leave the
current extent. The 158 selected positions all increase the total endpoint-distance
proxy despite shortening the chosen seed connection.

The independent source/consumer relationships explain this tradeoff. F36's output
terminal and F25's input terminal share **(24,50)**. F25 then supplies F18 through
two short belt connections. Moving the F36/F25 pair up next to F48 greatly extends
both F25->F18 links. The first automatically selected candidate also obstructs the
F03->F00 return. The actual native proposed-position overlay makes this visible.

A direct read-only test moving F36 by (+10,-15) fails immediately with
`local-shared-terminal-move`, entity `belt:24,50`; routing is never attempted.
`closeTerminalGroups` therefore mandates F36/F25 membership closure. The existing
port reassignment family also excludes shared anchors. This coordinate-identity
restriction, rather than a recipe obligation to move the two facilities together,
is the next internal contract to replace. The proposed F36-only destination has not
been proven simultaneously routable; the diagnostic establishes the early restriction,
not success after removing it.

## Bridge-release capability and correctness

`releaseBridges` validates up to eight explicit existing catalog bridges with two
terminal-free, straight, equal-rate owner paths. A private draft removes component
membership and releases the former cross-network coincidence anchor. Both owner
chains are re-decomposed and rerouted. They cannot silently remain rigid when their
endpoints receive equal translations. Splitters/convergers cannot be released.
New legal intersections use the existing catalog-backed bridge route engine; removed
IDs cannot be recycled within the transaction. Failure preserves the old complete
factory. Explicit chain IDs/priorities refer to the post-release decomposition, also
used by replay and failed-endpoint visualization.

The standalone actual-factory audit checks the untouched baseline, each of five
bridges and all five together. All twelve near/far restored candidates pass complete
original-graph validation. Six unrestored removed-bridge controls are rejected. Thus
deleting a bridge is not accepted merely because its body disappeared. The two all-
bridge controls reach 2679 / 438 / 38; the others reproduce 2679 / 438 / 39. The
selected all-bridge near result is rendered and independently audited again.

The area-candidate replay tries affected/belt/all rip-up, near/far orders and failed-
chain priority changes. The initial replay has 29 attempts; the incident-only replay
has 20. Neither finds a candidate. These are bounded route-order failures, not global
infeasibility or evidence that the central gap is necessary.

## Verification and visual inspection

207 Node tests pass; the final local suite has 37 passing tests. New tests cover
bridge release/reconstruction, both owner paths, illegal component types/IDs,
ownership bounds, no rigid carry of released owner paths, deterministic contact
proposals and accurate exhausted-enumeration accounting. Workspace check and 524
Rust tests pass, with two existing ignores. No Rust changes or release rebuild.

Personally inspected both original and incident-only rejected close-ups and the new
complete incumbent. The red dashed bodies are explicitly rejected proposals over the
unchanged factory, not an accepted partially routed output. Native pages pass six
viewport/theme checks and pan/zoom/Fit. Selected-factory original-graph and bridge
negative controls are recorded in `coalescence-results.json`.

Policy/design comparison: this remains JS research on transactional improvement of a
complete factory, preserving original rates, all facilities, independent validation and
area-first selection. The production frontier constructor and isolated exact solver
are unchanged. Geometry proposals do not introduce a production canvas limit, game
constant, rule hardcoding or a minimum-area claim. No Pumpkin or Rust cutover.

## Next milestone

Use stable terminal identity to separate coincident facility ports when their owners
move differently. A zero-length source/sink connection must be able to become a
nonzero chain while retaining its exact rate and network semantics. Do this as one
final-state transaction; do not invent temporary game coordinates or promote a worse
intermediate factory. Then rerun singleton, group and link-coalescence area candidates.
Inspect whether F36 can move without dragging F25 and whether the full bounds actually
contract. This specific next step does not replace the broader recipe/product goal.

## Reproduction

Use the normal graph, five catalog, release-validator and fresh output-directory flags
with local `run.mjs`: `--mode move --move-family coalescence --bridge-policy release`
(or `retained`) `--rounds 3 --move-limit 512 --endpoint-policy terminal-flexible
--junction-policy downstream --max-new-crossings 8 --layer-policy independent`.
Baseline: `component-results.json`. `priority-probe.mjs` replays stage zero of either
report with catalog directory and a fresh output directory. `bridge-release-audit.mjs`
accepts baseline, original graph, catalog directory, validator and output-directory.
JSON carries exact requests and source/input hashes; stdout is completion-only.
