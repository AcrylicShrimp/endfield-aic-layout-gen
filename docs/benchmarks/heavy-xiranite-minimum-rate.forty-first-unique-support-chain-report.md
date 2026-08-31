# Heavy Xiranite Minimum-Rate Unique-Support Chain Report

## Result

Recursive exact grid propagation reduces the controlled cumulative Phase 2 search tree beyond the
accepted terminal-only rule, but its additional runtime cost consumes the gain.

Across two repeated release runs, relative to terminal-only grid propagation:

- backtracks, conflicts, and learned clauses fall from 8,806 to 8,577 (`2.6%`);
- native solver propagations fall from 7,219,908 to 7,081,503 (`1.9%`);
- forced predicates rise from 90 to 361;
- the longest forced chain is 23 cells;
- maximum explanation size rises from nine to 176 predicates; and
- median first-witness time changes from 3,711 ms to 3,719 ms (`0.2%` slower).

The exact inference is therefore rejected as a runtime improvement in its current broad-wakeup
implementation. The terminal-only propagator remains the accepted diagnostic performance boundary.

## Repeated Comparison

| Metric | Terminal run A | Chain run A | Terminal run B | Chain run B |
| --- | ---: | ---: | ---: | ---: |
| First witness | 3,723 ms | 3,713 ms | 3,698 ms | 3,725 ms |
| Branch decisions | 43,545 | 43,543 | 43,545 | 43,543 |
| Backtracks | 8,806 | 8,577 | 8,806 | 8,577 |
| Conflicts | 8,806 | 8,577 | 8,806 | 8,577 |
| Learned clauses | 8,806 | 8,577 | 8,806 | 8,577 |
| Native solver propagations | 7,219,908 | 7,081,503 | 7,219,908 | 7,081,503 |
| Forced grid predicates | 90 | 361 | 90 | 361 |
| Maximum support chain | 0 | 23 | 0 | 23 |
| Maximum grid reason | 9 | 176 | 9 | 176 |
| Validation | passed | passed | passed | passed |

Both cases preserve the exact feasible set and add no decision variables. The diagnostic search
stops at its first witness, so the different secondary objective vectors are not a quality
comparison.

## Exact Rule

Starting at a selected demand, the propagator repeatedly forces an incoming material arc only when
that arc is the sole remaining physical predecessor and the current cell cannot be a supply. It
stops at a supply, branch, missing support, or repeated cell. It never chooses between multiple
possible arcs.

Each explanation contains the selected demand plus blocked incoming alternatives and unavailable
local supplies accumulated along the forced suffix. The reason grows with the chain, not the whole
grid.

Pumpkin `debug-checks` exposed an initially incomplete proof implementation: negating a forced arc
created a zero-support state, but the propagator did not report that state as a conflict. The final
implementation explicitly propagates a selected demand to false when its accumulated required
chain has no possible predecessor. All five controlled grid tests then pass Pumpkin's reason
validation, including alternative-predecessor and interior-branch cases.

## Interpretation

The deterministic search reduction proves that interior unique-support facts are useful. The flat
elapsed result shows that rescanning every selected demand chain after broad layer events is too
expensive for the small additional pruning obtained here. The next exact implementation question is
event scheduling or watched support, not a stronger global graph inference.

Any follow-up should preserve this chain's proof rule while waking only when a demand becomes
required or a possible arc or local supply loses support. It must be compared against both the
terminal-only runtime boundary and this chain's deterministic search tree.

## Artifacts

- `heavy-xiranite-phase2-unique-support-chain-grid-5s/summary.json`
- `heavy-xiranite-phase2-unique-support-chain-grid-repeat-5s/summary.json`

