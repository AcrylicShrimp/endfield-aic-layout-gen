# Atomic multi-group relocation: verified capability, no incumbent improvement

## Outcome

The selected factory remains **47x57 = 2,679 area / 444 transport cells / 45 turns**.
The incumbent witness hash is unchanged. The new engine can execute simultaneous
different translations and restore complete actual-catalog factories, but this initial
exchange/reverse-order proposal family does not improve the full score.

This is a negative quality result with a verified implementation capability, not a
completed packing solution. Keep the existing factory and do not port this experiment
to Rust based only on successful routing. The full research/product goal remains open.

## Contract cutover and implementation

Move schema 3 replaces `move` with `moves`, an array of up to four disjoint rigid
translation primitives, at most sixteen total facilities. Single-group moves use the
same contract. Old schema-2 replay inputs fail with an explicit migration diagnostic;
historical reports remain reproducible at their recorded commits. There is no sequential
move adapter or intermediate occupancy state.

One final-state movement plan computes all facility/terminal poses. Shared terminal
owners must have identical shifts. Nonterminal clusters move only when internal or
downstream ownership gives one common shift. Chains whose endpoints receive the same
shift retain their interiors; cross-group chains with different shifts are rebuilt.
New collisions among carried same-layer cells are rejected, while preexisting shared
cell identity remains legal. Facility bodies block both layers. Private drafts and
complete independent validation preserve the incumbent on every failure.

The proposal family starts with graph-derived groups and singleton facilities, then
closes them transitively over actual shared terminal ownership. This closure is necessary
for retained-port movement, not a permanent semantic grouping of recipes. It excludes
fixed external ownership and respects the membership budget. Pair exchanges and bounded
axis-order reversals use actual body collision checks, not solid group bounding boxes.
Ranking uses body envelope and endpoint-distance proxies; final selection uses the full
routed area/cells/turns tuple. Pair enumeration is bounded and separately reported.

The native rejected-move overlay now renders every schema-3 primitive. Generic replay
retains near/far, rip-up, failed-chain priority and paired-port controls. The optional
`--audit-unimproved 0..64` independently validates a bounded sample of routed but worse
candidates without promoting them. This distinguishes valid alternatives from merely
unvalidated route results and keeps the default validation budget unchanged.

## Experiments

All start from `constructive-embedding-compaction/layer-results.json`, use independent
layers, terminal-flexible endpoints, downstream junction ownership, three-round limits
and complete-factory validation. No experiment improves the incumbent.

| Family | Positions | Total attempts | Outcome |
| --- | --- | --- | --- |
| Single-group cone control | 128 | 330 | Incumbent retained |
| Initial non-closed group pairs | 74 | 222 | All 148 relocation attempts fail routing |
| Terminal-closed pairs | 128 of 248 | 330 | Four relocation requests route, all worse |
| All closed positions, paired ports, eight new bridges | 248 | 1,588 | Four relocation requests route, all worse |

The closed family has 88 groups. It examines all 3,828 pairs without reaching the 20,000
pair limit. Of 21,436 in-extent poses, 20,667 fail body collision; other screens reject
177 endpoint/body, 85 terminal, 197 fixed-anchor and 56 carried-cell conflicts. Eighty-eight
groups yield 248 surviving positions. Every selected pair increases the endpoint-distance
proxy, indicating that simple exchanges are mostly poor proposals for this incumbent.

The current-source expanded repeat takes 23.62s, executes the same 1,588 attempts and
validates all four routed relocation requests. They produce two distinct final witnesses
after compaction, at 2,679 / 466 / 53 and **2,679 / 465 / 53**. No validation rejection
occurs. Timings are experiment batches, not a controlled throughput benchmark.

## Visual investigation

First inspected a rejected exchange of F32/F45 and F33/F46. Its new bodies fit, but
restoration fails at the fixed merger interface. Six affected/belt/all near/far controls,
fourteen distinct failed-chain priority trials and two paired-port trials all fail.
The failed chains individually have body-only paths of 2, 7, 10, 11 or 18 steps. This
does not prove simultaneous routability, required bridge count or global infeasibility.
The red overlay shows proposed bodies on an unchanged validated factory, not an accepted
partially routed result.

Then independently audited and personally inspected the full and close-up render of a
successful but non-improving compound move: F26 moves up six rows, F45 down three before
compaction. The final geometry preserves all 59 facilities and has nine components.
The F26 area acquires extra bent purple connections and two new bridges around the
retained interface. It occupies part of the interior pocket without shrinking the used
outer bounds. Physical cells increase **444 -> 465**, turns **45 -> 53**. It is explicitly
not promoted. Eight original-graph controls pass for this alternative; its witness hash
matches the final-source repeat exactly. Both native diagnostics pass six viewport/theme
checks plus pan/zoom/Fit.

The measured next gate is rebuilding the affected interface with the surrounding group
poses: existing splitter/merger positions, boundary port order and internal group shape
still restrict the tested family. Do not infer that adding more pair swaps, or passing
the same fixed interface to a solver, will remove the void. Evaluate coordinated interface
resynthesis or a bounded joint placement/interface oracle. Pumpkin remains an optional
idea rather than a mandated switch; no new solver dependency has been introduced.

## Verification and reproduction

- 198 Node tests pass. New coverage includes simultaneous exchanges that fail as either
  sequential move, identical/different anchor shifts, shared terminal closure, fixed
  external ownership, moved-body/carried-cell collisions, legacy request rejection,
  detached metadata, deterministic proposals and pair budgets. Existing actual-catalog
  cycle, port, routing and geometry suites remain green.
- `cargo check --workspace` and `cargo test --workspace` pass: 524 tests, two existing
  ignores. No Rust files changed.
- Current-source control and expanded runs retain the original audited incumbent hash.
  Four non-improving requests independently validate; the representative alternative
  also passes eight separate original-graph controls with matching current-source hash.
- Policy review: a bounded complete-layout improvement transaction, no production W/H
  constraint or global exact fallback, runtime catalogs and independent acceptance. The
  immediate-routing constructive frontier policy remains unchanged.

Run `run.mjs` with the original graph/catalog/validator flags and baseline above:

```text
--rounds 3 --mode joint --move-family compound --move-limit 512
--endpoint-policy terminal-flexible --junction-policy downstream
--max-new-crossings 8 --port-policy paired --layer-policy independent
--audit-unimproved 8 --output-directory NEW_PATH
```

Control: `--move-family cone --move-limit 128 --max-new-crossings 4 --port-policy retained`.
Replay a rejected schema-3 stage with `priority-probe.mjs --report PATH --stage INDEX
--catalog-directory data/game/normalized --max-new-crossings 4 --port-policy paired
--output-directory NEW_PATH`. Historical schema-2 reports are not silently converted.
The initial non-closed family was superseded during this slice; its output is recorded
as historical evidence, not another permanent runtime mode.

Machine-readable result and artifact references: `compound-results.json`.
