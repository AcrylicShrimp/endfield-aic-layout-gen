# Dependency-cone movement and downstream junction ownership

## Outcome and controlled comparison

The complete factory retains **47x61 = 2,867 area**. Physical transport cells improve
426 -> 410 and turns 38 -> 34. All 59 facilities remain present. Eight independent
original-graph controls pass and their unchanged-witness hash matches the current-source
repeat. This is a small secondary-score improvement, not a solution to the visible void.

All four cells below start at the same 2,867 / 426 / 38 incumbent, use three joint
rounds, 256 move positions per round, both near/far routing orders and terminal-flexible
endpoints. Each executes 1,752 attempts. Trials ran concurrently, so elapsed times are
batch observations rather than controlled performance comparisons.

| Move proposals | Junction ownership | Area / cells / turns | Batch seconds |
| --- | --- | --- | --- |
| Original groups | All terminal neighbors selected | 2,867 / 411 / 36 | 40.44 |
| Dependency cones | All terminal neighbors selected | 2,867 / 411 / 35 | 46.51 |
| Original groups | All outgoing consumers selected | 2,867 / 411 / 36 | 30.24 |
| Dependency cones | All outgoing consumers selected | 2,867 / 410 / 34 | 40.98 |

Most of the sixteen-cell gain also occurs with the original group family under the
larger 256-position budget. The two new mechanisms together beat that control by only
one transport cell and two turns. Keep them opt-in research, not an established Rust
production cutover. A current-source repeat takes 37.06s and reproduces the selected
witness hash exactly; generated detail metadata can differ across output directories.

Machine-readable incumbent and artifact references: `cone-results.json`.

## Implementation

`dependencyCones` grows full predecessor/successor layers from each actual graph node,
up to four layers and sixteen facilities. It does not truncate an oversized layer or
hardcode recipe names. Existing cyclic, pair and short-serial groups remain controls.
The initial actual candidate set increases from 91 to 298 groups; the 256-position
portfolio includes 102 cone proposals under internal junction ownership.

The seven-facility F19/F18/F13/F12/F08/F07/F03 fork is now an explicit move candidate.
The old internal-only junction rule leaves its two input bridges stationary. The new
downstream rule carries a complete nonterminal anchor cluster when all its outgoing
boundary terminals are selected facility inputs. Incoming external supply paths are
rerouted; external terminals never move. Component rotations, ports, flows and all
unaffected geometry retain their existing contracts. Every promoted factory independently
validates. Both new proposal families preserve the area/cells/turns score order.

## Visual failure investigation

Personally inspected the final full layout and product-side close-up. The void below
F00 remains conspicuous. Occupied XY is 1,517; empty XY is 1,350. The largest empty
rectangle is 20x8=160 (previously 21x8=168). A fill ratio alone would mischaracterize
the reduced wiring as worse packing.

Also rendered and personally inspected a rejected move: the bottom F37/F48/F36/F25
bank translated by (+13,-16) into the void. Red dashed bodies show proposed positions;
red rings show failed chain 47 endpoints. Solid geometry is the unchanged validated
incumbent, not a partially routed accepted factory. The bodies fit, but routing F25's
two outputs to F18 across the existing process links fails under the tested embedding.

Replayed six rip-up/order controls: affected chains only, all belt chains, and all
chains, each near/far. None reconstructs a complete candidate. Then prioritized failed
chains before the remaining near/far order, deduplicating the resulting priority queues.
Eighteen distinct order trials fail before the 64-trial cap. Failure moves among chains
47/48 (F25 -> F18), 64 (F07 -> F03), 67 (F03 -> F00), 70 (final export) and 22 (a source
bank output). This is not merely the original short-route-first choice repeating.

With all routing obstacles removed, both displaced F25 paths individually exist in
28 steps. This body-only relaxation proves neither simultaneous routability nor a valid
factory. The measured conclusion is restricted: broader rip-up and these order retries
do not solve the move with the retained port identities and junction topology. It does
not prove that a new bridge is necessary, that all route orders fail, or that the factory
is globally infeasible.

Next reconstruct the affected interface jointly with the proposed bank position: allow
port reassignment and a fresh structured crossing alternative, rather than only translating
existing components. Compare against a bounded joint routing/port oracle if useful. Retain
the complete incumbent while attempting this transaction. No additional recipe/product
stage is declared complete.

## Reproduction and verification

Use `run.mjs` with the common runtime catalog and validator flags, graph
`target/adjacent-reciprocal-bank/graph.json`, baseline
`artifacts/benchmarks/constructive-reciprocal-banks/results.json`, and:

```text
--rounds 3 --mode joint --move-limit 256 --endpoint-policy terminal-flexible
--move-family cone --junction-policy downstream --output-directory NEW_PATH
```

Substitute group/cone and internal/downstream for the matrix. Audit the chosen report
with `constructive-factory-closure/audit.mjs` against the original
`constructive-chain-blocks/gap-1-placement.json`. Current implementation hashes are in
`target/cone-current-source/report.json`; selected witness identity matches the audit.

The generic replay CLI contains no facility-label selection:

```sh
node docs/benchmarks/constructive-local-reembedding/priority-probe.mjs \
  --report target/cone-current-source/report.json --stage 74 \
  --catalog-directory data/game/normalized \
  --output-directory target/cone-current-replay
```

Use a fresh output path for repetitions. It emits detailed JSON and a native diagnostic
HTML; stdout is a short completion record. Hypothetical successful replay candidates are
explicitly unvalidated and cannot replace an incumbent without independent validation.

- 141 Node tests pass; new checks cover layer budgets, input immutability, graph order,
  cycles, the actual product fork, downstream junction ownership, external boundaries,
  invalid policy/priority inputs and priority not expanding the rip-up set.
- `cargo check --workspace` and `cargo test --workspace` pass: 524 tests, two existing
  ignores. No Rust files changed.
- Eight original-graph audit controls pass; all current-source and repeated witness
  identities were checked before promotion.
- Actual CLI replay and six viewport/theme combinations pass, including pan/zoom/Fit.
  Full, close-up and rejected-proposal renders were personally inspected.

Policy review: bounded complete-layout improvement only, no production W/H constraint,
no global exact fallback, runtime game data retained, and independent validation before
promotion. The production immediate-routing frontier contract in the constructive planner
design is unchanged. These are useful research alternatives with limited measured gains.
