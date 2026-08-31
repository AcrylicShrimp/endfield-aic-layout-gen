# Phase 3 Prior-Terminal Subset Report

## Result

The next Phase 3 cliff is the joint state of two belt-demand terminals on the preceding final-target
facility. Fixing either terminal's reference port alone does not cross the five-second cliff. Fixing
both changes the selected state into a 185 ms infeasibility proof. Every superset containing that
pair also proves infeasible in 178--185 ms.

The minimal collapsing mask is `0xC`.

| Bit | Role | Material | Rate | Reference port |
|---:|---|---|---:|---|
| 0 | demand | pipe material | - | `input-pipe-5` |
| 1 | supply | belt material | - | `output-belt-4` |
| 2 | demand | `item-xiranite-powder` | 1/2 | `input-belt-1` |
| 3 | demand | `item-xiranite-powder` | 1/2 | `input-belt-2` |

Bits 2 and 3 are two demand lanes in the same shared `item-xiranite-powder` belt commodity network.
The material and rate identity was cross-checked against the validated Phase 2 terminal report with
the same stable wiring-edge IDs.

## Complete 16-subset result

| Mask | Fixed bits | Outcome | Search | Decisions | Backtracks | Conflicts | Propagations |
|---:|---|---|---:|---:|---:|---:|---:|
| `0x0` | none | Unknown | 5,006 ms | 28,924 | 2,654 | 2,653 | 2,820,808 |
| `0x1` | 0 | Unknown | 5,007 ms | 29,446 | 2,682 | 2,681 | 2,862,662 |
| `0x2` | 1 | Unknown | 5,007 ms | 29,025 | 2,660 | 2,659 | 2,829,296 |
| `0x3` | 0,1 | Unknown | 5,007 ms | 29,402 | 2,679 | 2,678 | 2,859,794 |
| `0x4` | 2 | Unknown | 5,006 ms | 29,672 | 2,772 | 2,771 | 2,887,407 |
| `0x5` | 0,2 | Unknown | 5,006 ms | 29,769 | 2,786 | 2,785 | 2,904,273 |
| `0x6` | 1,2 | Unknown | 5,008 ms | 29,690 | 2,774 | 2,773 | 2,890,483 |
| `0x7` | 0,1,2 | Unknown | 5,006 ms | 29,139 | 2,724 | 2,723 | 2,827,572 |
| `0x8` | 3 | Unknown | 5,007 ms | 32,572 | 2,647 | 2,646 | 2,848,505 |
| `0x9` | 0,3 | Unknown | 5,007 ms | 32,696 | 2,657 | 2,656 | 2,860,692 |
| `0xA` | 1,3 | Unknown | 5,007 ms | 32,611 | 2,649 | 2,648 | 2,852,340 |
| `0xB` | 0,1,3 | Unknown | 5,007 ms | 32,249 | 2,613 | 2,612 | 2,811,587 |
| `0xC` | 2,3 | Proven infeasible | 185 ms | 370 | 28 | 29 | 278,573 |
| `0xD` | 0,2,3 | Proven infeasible | 179 ms | 370 | 28 | 29 | 278,575 |
| `0xE` | 1,2,3 | Proven infeasible | 178 ms | 370 | 28 | 29 | 278,571 |
| `0xF` | 0,1,2,3 | Proven infeasible | 178 ms | 370 | 28 | 29 | 278,573 |

No case produced an incumbent. The pair separation is exact across the complete subset lattice: all
12 masks missing at least one of bits 2 and 3 time out, while all four masks containing both prove
infeasible quickly.

## Why four terminal variables are still expensive

These are not four Boolean variables. Each terminal selects among several compatible physical
ports. A complete four-terminal assignment has approximately 125 combinations for this facility.
Each assignment also changes endpoint cells and directions for the much larger 64,471-variable
placement-routing-flow model.

The monolithic solver does not currently enumerate the high-level port tuple first and then solve a
clean routing subproblem. It interleaves those choices with low-level route, item, topology, and
flow decisions. The no-port case makes 28,924 decisions and 2,653 conflicts without resolving the
selected state. Once the decisive two port values are supplied, the same model needs only 370
decisions and 29 conflicts to prove the contradiction.

## Next exact experiment

The smallest justified next partition is not all four terminal values. It is the Cartesian product
of legal port values for the two same-network belt demands identified by bits 2 and 3, while:

- retaining fixed preceding placements;
- leaving the pipe demand, belt supply, every other preceding port, and all routing state free; and
- preserving the five-second per-case budget.

This is an exact pair-value portfolio. Exhaustively running every legal pair loses no solution:

- if most pairs quickly prove infeasible or produce witnesses, a solver-orchestration partition on
  coupled same-network terminal groups is promising;
- if most pairs remain unknown, the pair values only gate a deeper endpoint-to-routing cliff; and
- the result tells whether a future semantic propagator should reason about same-network facility
  demand pairs rather than isolated terminals.

No production partition or new propagator is approved by this report alone.

## Artifacts

```text
/tmp/aic-phase3-prior-terminal-subsets
```

The CLI generated `summary.json`, `summary.html`, and 16 standalone HTML failure artifacts.

## Verification

```text
cargo fmt --all
cargo check --workspace
cargo test --workspace
cargo build --release -p aic-cli
git diff --check
```
