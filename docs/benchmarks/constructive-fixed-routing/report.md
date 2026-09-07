# Frozen-placement wiring: straight pairs, then alternate ports and A*

## Result

Heavy Xiranite's pipe-north placement stays frozen: all 59 original rectangles and rotations are
verified unchanged after subtracting the explicit search translation. **60 of 89 internal
requirements connect**, including both internal pipe requirements. Thirty succeed in the straight
stage and thirty in alternate-port/A* search. Twenty-nine internal and seven external requirements
remain pending. This is not a complete factory.

The retained witness independently validates with the existing constructive network validator:
11 commodity networks, 25 logistics components, summed network cell count 746 (not necessarily
unique physical tiles when networks cross). The used facility/network footprint is **57 x 101**,
starting at (-5,-2) relative to the frozen placement origin. Search canvas **84 x 130** includes
16-cell padding and is a diagnostic resource envelope, not a blueprint footprint or game limit.
`final_node` retains search coordinates and its bounds describe that envelope; `summary.json`
explicitly separates actual used geometry. No facility moved relative to another.

## Implementation boundary

`aic-fixed-routing-probe` is a separate explicit diagnostic executable. Normal production commands
do not invoke it. The accepted production design still constructs connected, incrementally routed
factories; this is the user's isolated frozen-geometry exception. Neither exact search nor a new
simplified routing model is used. Runtime catalogs are loaded on every run; frozen catalog hash
must match, and ports are projected afresh rather than trusted from visualization records.

The probe searches belts before pipes. For each layer:

1. Try unused opposed ports whose connection cells form an unobstructed straight line. A straight
   success is a single capacity-sufficient lane; shared/multi-lane cases go to general search.
2. For unresolved requirements, enumerate endpoint port pairs by connection-cell Manhattan distance
   and stable port IDs. Reuse existing A*, commodity attachments, splitters/convergers, crossings,
   shared-network capacity logic and independent validation. One pair is presented at a time:
   unselected alternative port domains are not treated as assigned corridor reservations.
3. A candidate is accepted only if its covered-route witness validates and no previously nonempty
   pending boundary attachment domain becomes empty. Domains include external requirements. Failed
   candidates never replace the current witness; other independent requirements are still attempted.

The straight stage reuses A* even for the already-clear line (the one-cell case returns immediately),
so A* counts include these cheap calls. This slice prioritizes semantic reuse over a duplicated
direct-route materializer. Each stage/requirement has its own 64-call budget; the CLI flag is
`--max-astar-per-connection` and report field is `per_stage_connection_astar_limit`. Padding is 16;
the diagnostic workspace safety ceiling is 250,000 cells. No local/global infeasibility claim follows.

## Confirmed next bottleneck

There are **832 A* calls and 526 future-domain candidate rejections**, with zero candidate witness
validation rejections. All initial boundary domains are nonempty. A nonempty domain only means a
local attachment exists, not that every remaining connection can coexist or reach its destination.

The first unresolved report entry, **F56 -> F51** (seed return), considers 20 port pairs. All twenty
find a geometrically valid candidate but lose the last attachment option of at least one pending
plant feed, **F51 -> F56** or **F50 -> F56**, so all are rolled back. The output lists the affected
requirement IDs. The pictured red dashed line is the failed logical requirement, not a rejected
physical path; gold/blue geometry is the retained validated witness only.

Current A* optimizes a single path under its current obstacle mask; future-domain preservation is
checked afterward. A rejected shortest path does not imply that a longer path for the same port
pair is impossible. The new regression `rejects_a_shortest_path_that_consumes_the_last_pending_external_port`
exhibits this limitation on three simple facilities. **Next priority: feed last-domain/approach
preservation into route search or bounded path retry before concluding that relocation is needed.**
Do not hard-reserve every unused port or the former three-cell escape corridor again: preserve
real remaining choices, not every alternative simultaneously. Shared-cycle feeds may subsequently
need a small grouped port/routing transaction or local rip-up. Local relocation remains a later
precisely scoped candidate search, not an unapproved global re-placement fallback.

## Timings and profiling

Three sequential fresh release processes on warm files, without concurrent build/test load:
probe median **9,349.14 ms**, range **9,315.86–9,349.24 ms**. These measure fixed routing and its
validation/domain work, not placement, catalog loading or report serialization. CLI wall times
are recorded separately in `timing.json`. An additional instrumented run takes 9,798.30 ms; its
final witness hash equals every uninstrumented run's. Detailed profile is in `profile.json`.

| Operation | Calls | Exclusive ms | Inclusive ms |
| --- | ---: | ---: | ---: |
| A* | 832 | 2,503.75 | 2,503.75 |
| Attachment enumeration | 71,988 | 1,763.85 | 3,833.84 |
| Shared path lookup | 1,771,477 | 1,331.17 | 1,331.17 |
| Shared path index build | 9,091 | 738.83 | 738.83 |
| Validation setup | 588 | 629.22 | 629.22 |
| Boundary refresh | 1,316 | 546.21 | 4,360.41 |
| Crossing options | 832 | 367.57 | 367.57 |

Inclusive times overlap: boundary refresh includes attachment enumeration and shared-path work.
Do not add them. The result identifies both wasted rejected searches and expensive repeated
domain refresh; it does not establish a layout infeasibility or justify dropping validation.
State mutations are serial/deterministic; no parallel candidate search is claimed.

Output is compact JSON to file (864,087 bytes in run 0), with a short completion on stdout.
The standalone HTML is about 1.17 MB. The large historical construction log is not reproduced.
Raw runs and profile: `/tmp/aic-fixed-routing.DraRtY/`. `routing.json`, `summary.json`, `timing.json`
and `profile.json` preserve this milestone's witness, input/binary hashes and measurements.

## Reproduction

```sh
cargo build --release -p aic-cli --bin aic-fixed-routing-probe
target/release/aic-fixed-routing-probe \
  --placement artifacts/benchmarks/constructive-pipe-north/heavy.json \
  --facilities data/game/normalized/facilities.json \
  --items data/game/normalized/items.json \
  --transports data/game/normalized/transports.json \
  --components data/game/normalized/logistics-components.json \
  --report-output NEW_ROUTING_JSON
node docs/benchmarks/constructive-placement-only/route-view.mjs \
  artifacts/benchmarks/constructive-pipe-north/heavy.json NEW_ROUTING_JSON NEW_HTML
```

Add `--profile` for operation counters/timings in the JSON file. Output paths must be new. Exit zero
means the diagnostic ran, not that every requirement connected. Factory validation stays false,
even if all internal edges happen to connect, because external routing is deliberately pending.

## Verification and publication

- `cargo fmt --all -- --check`, `cargo check --workspace`, `cargo test --workspace` pass.
  514 Rust tests pass, with two pre-existing research ignores. Five new probe regressions cover
  straight cells, offset detours, explicit initial obstruction, invalid input/resource limits and
  transactional rejection of the last external attachment being consumed.
- Eleven Node placement/CLI tests and six Python tool tests pass.
- Manual release probe: three unprofiled runs and one profile produce identical final witnesses.
  Every original facility coordinate/rotation is checked against the recorded translation.
- Browser preview passes at 1400/736/360 px in both themes, with failure selection, port display
  and no horizontal page overflow. Final page rechecked with affected feed labels and mobile widths.
- Existing tailnet server publishes `http://100.114.24.86:8765/fixed-routing.html`; previous placement
  pages remain unchanged. Actual browser-rendered overview and failure screenshots are saved as
  `target/shared-results/fixed-routing-overview.png` and `fixed-routing-failure.png` for chat output.

This milestone ends at an evidence-backed routing bottleneck. No local relocation is implemented.
