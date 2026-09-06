# Interface-first assembly: completion restored, area incumbent retained

Status: verified research milestone. Both source-cut controls construct a complete
59-facility Heavy Xiranite factory. Neither beats the existing complete incumbent:
**104x31 / 3,224 area / 775 transport cells / 119 turns**. No production cutover.

## What changed

Instead of packing all source banks before discovering their connecting geometry,
discover common ready belt consumers and compose those source banks, consumer slots,
port assignments and structured interconnects together. Other banks remain independent.
Every accepted compound validates its physical geometry, covered recipe margins and
pending interface. Private rejected alternatives do not modify retained regions.

The group count is graph-derived, not fixed at two. The actual graph still converges to
two 18/35-facility belt regions before mixed-input closure. This is not implementation
of an occupancy threshold: it deliberately measures interface-first full frontier growth
before selecting a new stop policy. Do not present it as the complete user proposal.

Sources retain runtime-allowed rigid poses and small portfolios of equal-membership
envelopes. The seven small source banks support cuts of two, three or four members.
The larger bank's two/four/five-member cut variants cannot expose the free east-facing
interface required by this template family. Its independently validated seven-member
retained parent is the supported lookahead alternative in both controls. This is an
explicit restriction, not evidence that smaller SCC seeds cannot be wired in the game.

## Measured controls

Both controls use shape limit/beam four, eight final packing alternatives, two closure
workers and the existing simple/two-sided closure with 64 placement candidates. Fixed
logical margins and all runtime catalogs are shared. Three rounds of existing group
relocation and terminal-flexible A* use a 128-position move limit per round.

| Source cut | Initial bank membership | New completions | Raw complete area / tiles / turns | Refined area / tiles / turns |
| --- | --- | --- | --- | --- |
| Minimum supported | Seven pairs plus seven-member parent | 1/8 | 7,821 / 1,379 / 133 | 3,888 / 918 / 87 |
| Maximum supported | Seven four-member banks plus seven-member parent | 1/8 | 7,488 / 1,700 / 221 | 5,180 / 1,455 / 139 |

Both finish through shelf alternative five. Some other candidates cover all 59 facilities
but fail true external-boundary completion; they are not counted as complete. The former
occupied-first experiment completed zero of 32 attempts, but these are different candidate
families, not paired samples or a success-probability estimate.

The first construction observations were 24.438s / 25.643s; current-source repeat runs
were 38.703s / 40.020s under concurrent verification load. They reproduce the complete
witness hashes exactly. These are whole eight-candidate experiment times, not the cost
of constructing one factory or a CPU speedup benchmark. Refinement takes 41.481s / 48.249s
for 1,020 / 1,065 attempts, respectively. Both refinements validate all 59 facilities,
131 recipe margins and seven true externals against the original, unmodified graph.

The smaller refined result occupies 1,978 XY cells inside 3,888 area: 1,910 empty cells
(49.13%), with a largest empty rectangle of 5x39. The other has 2,677 empty cells inside
5,180 (51.68%), including a 26x25 void. A more square outline alone is not improvement.

## Visually inspected next bottleneck: crossing geometry

Native raw, refined and crossing-focus renders were personally inspected. The small
source compounds are compact, but composition grows a large central routing band.
The larger inherited seed still has conspicuous nested return loops. The refined
minimum-cut factory retains long vertical corridors beside the central seed. Neither
the interconnect geometry nor the resulting assembled shape is satisfactory.

The user correctly identified inefficient crossings. `permutationModule` implements
adjacent lane exchanges using fixed 6x6 reservations, lane pitch three, and two detouring
paths per exchange. Independent exchanges already share stages; it is incorrect to say
every exchange is serialized. Dependencies still create a wide staged network, and
unchanged lanes pass through each stage. Port approaches and tree reservations add
further width. These are construction-template choices, not minimum game dimensions.

In the maximum-cut first retained four-consumer compound, twelve exchanges occupy six
stages: the abstract permutation core is **36x36 before compaction**, inside an eventual
80x56 compound. The smaller two-consumer compound uses two exchanges/two stages and a
12x18 abstract core. Do not attribute those pre-compaction sizes to the final refined
geometry. Existing strip contraction removes event-free columns, but protects turns and
bridges; it cannot by itself redesign the repeated U-shaped exchange paths visible in
the crossing-focus image. Group A* refinement also does not resynthesize bridge topology.

Next experiment: hold exact lane commodities, capacities and boundary order fixed, and
compare a smaller exchange embedding or direct orthogonal crossing arrangement against
the current staged template. Independently verify directional bridge arms, separate
channels, clearance and all physical cells before whole-factory integration. Separately
compare source/consumer/port ordering to avoid unnecessary exchanges. Do not assume
template pitch, six-cell width, or every current crossing is necessary. Whole connected
area remains the acceptance objective; bridge count and fill are not substitutes.

## Verification and reproducibility

- 131 related Node tests pass: 125 across the twelve construction/improvement directories
  plus six cycle-block tests. Five new tests cover pose geometry, catalog rotations,
  external-boundary rejection, source cuts, ownership and graph-derived group counts.
- `cargo check --workspace` passes; `cargo test --workspace`: 524 pass, zero failures,
  two existing ignores. No Rust edits or dependency changes.
- Eight independent original-graph audit controls pass for each refined factory. The
  summary checks audit identity against the actual refined witness hash.
- Four native browser captures pass six theme/viewport checks each and zoom/pan/Fit
  checks. Raw and refined whole factories and a raw crossing close-up were inspected.
- `results.json` preserves compact scores, source cuts, wave geometry, repeat-run hash
  checks, refinement provenance and audit outcomes. Full trial logs remain under
  `target/frontier-interface-{min,max}-final` and `...-{min,max}-refined`.
- One preliminary maximum-cut refinement was launched with the transport catalog in the
  component argument. Initial validation failed before search; that invocation is
  excluded. The correctly configured independent run provides all reported max results.

Run `run.mjs` with the CLI documented in `contract.md`, graph
`target/scc-stop-grow-final/graph.json`, inventory in the same directory, and incumbent
`docs/benchmarks/constructive-shape-packing/full-matrix.json`. For refinement use the
successful attempt's `closure-5/report.json` as baseline, not the top-level report which
selects the retained incumbent. `summarize.mjs` verifies and reduces both controls.

Native artifacts: `target/shared-results/bound-interconnect-closure-frontier-min-refined`
and `...-max-refined` (`.html` / `.png`); the diagnosed crossing close-up is
`bound-interconnect-closure-frontier-max-crossing.png`, from the raw maximum-cut factory.

Reviewed against the constructor design and repository policy: private routed growth,
catalog-backed rules, independent acceptance and complete-incumbent preservation remain.
This isolated belt-first research path does not change the normal production constructor.
Compact crossing reconstruction, stop/grow partition selection, inherited large-cycle
geometry, broader recipes and product stages remain open. No global infeasibility or
minimum-area claim, and the broader goal is not complete.
