# Phase 3 Prior-Port Facility Subset Report

## Result

The Phase 3 cliff localizes to the four ports of the preceding final-target facility instance. Fixing
that facility's four reference ports changes the selected state from a five-second unknown result
to a 225 ms infeasibility proof. Fixing either or both upstream facilities' ports does not cross the
cliff.

| Mask | Fixed preceding facilities | Fixed ports | Outcome | Search | Decisions | Backtracks | Conflicts | Propagations |
|---:|---|---:|---|---:|---:|---:|---:|---:|
| `0x0` | none | 0 | Unknown | 5,007 ms | 37,735 | 3,386 | 3,385 | 3,700,607 |
| `0x1` | liquid-xiranite-poly upstream | 4 | Unknown | 5,007 ms | 33,890 | 3,318 | 3,317 | 3,959,859 |
| `0x2` | xiranite-powder upstream | 4 | Unknown | 5,007 ms | 38,246 | 3,180 | 3,179 | 3,686,868 |
| `0x3` | both upstream facilities | 8 | Unknown | 5,006 ms | 38,313 | 3,217 | 3,216 | 3,834,532 |
| `0x4` | final target facility | 4 | Proven infeasible | 225 ms | 370 | 28 | 29 | 278,573 |
| `0x5` | final target + liquid upstream | 8 | Proven infeasible | 187 ms | 227 | 12 | 13 | 269,211 |
| `0x6` | final target + powder upstream | 8 | Proven infeasible | 211 ms | 370 | 28 | 29 | 278,925 |
| `0x7` | all three | 12 | Proven infeasible | 207 ms | 227 | 12 | 13 | 269,589 |

No case produced an incumbent. Every subset containing bit 2 proves infeasible quickly. Every
subset without bit 2 remains unknown for the full budget. This clean separation is stronger than a
small timing correlation.

## Stable facility mapping

| Bit | Preceding facility instance | Matching facility terminals |
|---:|---|---:|
| 0 | `/target/.../input:item-liquid-xiranite-poly:0` | 4 |
| 1 | `/target/.../input:item-xiranite-powder:0` | 4 |
| 2 | `/target:0` | 4 |

Instance IDs were sorted lexicographically before masks were generated. All eight subsets were
executed concurrently with independent five-second search budgets.

## Interpretation

The previous report identified a transition after all 12 preceding facility ports were fixed. This
experiment shows that eight of those equalities are unnecessary for the transition. The decisive
information is wholly contained in the final target facility's four port choices for this selected
state.

This is still a diagnostic infeasibility result. It does not prove that the final target facility's
port channel dominates every Phase 3 state, and it does not prove routing-only feasible cases are
cheap. It identifies the smallest facility-level block measured so far.

The next exact decomposition is the complete 16-subset lattice over those four terminal equalities,
with preceding placements still fixed and every other preceding facility port left free:

- if one terminal alone collapses search, inspect its endpoint-to-route support;
- if only a pair or larger set collapses search, inspect the network interaction among those
  terminals; and
- if only all four collapse, the missing propagation is collective facility endpoint state rather
  than one isolated port.

## Exactness

The subset masks are diagnostic restrictions copied from a validated Phase 2 incumbent. They are
not a production solver strategy. All masks are tested, and no routing, path, corridor, placement,
or port heuristic is introduced.

## Artifacts

```text
/tmp/aic-phase3-prior-port-subsets
```

The CLI generated `summary.json`, `summary.html`, and one standalone HTML failure artifact for each
mask.

## Verification

```text
cargo fmt --all
cargo check --workspace
cargo test --workspace
cargo build --release -p aic-cli
git diff --check
```
