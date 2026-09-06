# Geometry-driven translation groups

## Contact-closure experiment

The first contact-push portfolio retains 41x53 / 2,173 area and reduces physical
transport from 425 to 408 cells, with 40 turns. All 59 facilities and original graph
obligations validate. The same-incumbent cone control reaches 412 cells / 39 turns.
Both use joint mode, 256 moves, near/far orders, bounded vacant-port retries,
downstream junction ownership, incident bridge release and independent-layer compaction.

The push run uses three rounds, 6,785 routing attempts and 36.09s observed elapsed
time. Cone stops unimproved after two rounds, 5,606 attempts and 26.66s. These are
concurrent optimization-batch observations, not constructor latency or speedup claims.
Reports: `target/geometric-push-first/report.json` and
`target/geometric-push-cone-control/report.json`.

Each push round samples 4,096 poses across 160 seeds. Round one closes 3,284 groups,
of which 2,203 gain members; 1,112 distinct proposals pass geometry screening and
256 enter routing. Body bounds, fixed externals, member limits and existing movement
checks account for the recorded rejections. Counts do not prove omitted candidates
invalid or the neighborhood exhaustive. Same-layer terminal contact uses a position
index rather than scanning every other terminal for each translated terminal.

### What the new closure actually contributes

Stage 513 begins with F40 and discovers seven additional lower-bank facilities by
body contacts and one terminal contact: F49, F39, F36, F25, F18, F37 and F48. Their
one-cell rightward translation validates at 2,173 / 425 / 39. This proves that the
operator can construct and route a geometry-driven group not supplied as one logical
seed. However, that temporary selected state is a sibling, not an ancestor of the
final winner. Do not attribute the final wiring reduction to this grown group.

The final winner's main moves are smaller: F04/F01 right by one, F01 right by one
again, and F21 down two. The first is also found by the cone control. No area gain
is claimed from the contact-only seed family.

## Verification and visual review

236 Node tests, cargo check, and 524 Rust tests pass (two existing ignores).
Eight original-graph controls and sixteen crossing controls pass on the selected
factory. Tests cover body/terminal contact directions, layers, shared ownership,
recognized detachable pairs, fixed externals, membership and pose budgets,
determinism and input immutability. No Rust files or dependencies change.

Personally inspected the native full push and cone results and the grown lower-bank
group's focused view. All three pages pass six viewport/theme checks. Central empty
space and the long lower return remain. Selected viewer:
`http://100.114.24.86:8765/bound-interconnect-closure-geometric-push-first.html`.

Compared against AGENTS.md and the constructive-planner design: this is a bounded
private improvement transaction on a complete witness, not global facility-only
construction. Fixed external obligations, local release limits, existing movement
checks, full graph validation and area-first scoring remain unchanged.

## Next experiment

Contact propagation alone does not group separated facilities that jointly define
the used height. Add explicitly bounded facility-envelope support cohorts as seeds,
then apply the same closure and validation. Their potential benefit is coordinated
boundary movement, not a claim that body-envelope area equals the factory footprint.
The broader compactness, recipe breadth, cleanup and web goals remain unfinished.
