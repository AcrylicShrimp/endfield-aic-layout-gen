# Full-rate fixtures, removable crossings and a restricted obstruction

## Results

| Fixed fixture | Facilities | Consumer utilization | Bridges | Used bounds | Physical belt cells | Pending recipe terminals |
| --- | ---: | ---: | ---: | --- | ---: | ---: |
| Original merge/split demonstration | 5 | 1/4 | 1 | 35 x 22 | 129 | 0 |
| Dedicated full-rate pairs, fixed input order | 5 | 1 | 2 | 35 x 22 | 157 | 0 |
| Dedicated pairs, supplier-ordered input ports | 5 | 1 | 0 | 35 x 22 | 157 | 0 |
| One producer per item, two consumers | 4 | 1/2 | 1 | 35 x 18 | 117 | 0 |

All rows pass the unchanged independent Rust witness validator. They are complete local recipe
fixtures with external boundary supplies/outputs, not the full heavy-xiranite graph. Different
utilizations and facility sets are not like-for-like objective comparisons. The controlled full-rate
port-order comparison removes two components but does not reduce area or physical tile count.

## What is automatic and what is fixed

`dedicated.mjs` derives source utilizations and lane rates from runtime recipes and target fractions,
projects catalog ports, lowers the given paths and connects local external requirements. The
fixture files explicitly specify node positions, port IDs, target fractions and bend points.
Successful validation therefore proves that the specified construction works, not that a common
placement/routing engine discovered it. The original production constructor is unchanged.

`order-ports.mjs` is a shared but narrowly scoped transformation: order each consumer's belt inputs
by supplier output row, align a supplier feeding only that consumer when all its lane offsets agree,
and rebind the existing direct/one-column paths. It does not search arbitrary paths or certify a
remaining no-module candidate; independent validation is mandatory. Unsupported path classes stop
explicitly. The user correctly identified C1's original A1,A1,B order as causing unnecessary
crossings. B,A1,A1 plus a one-cell A1 shift removes both bridges at unchanged throughput.

## A different-item case that ordering cannot remove in this corridor

`two-supplier-fixture.json` uses one `grinder-plant-moss-powder-2-1` and one
`grinder-plant-moss-powder-3-1`, each feeding both instances of
`thickener-plant-moss-enr-powder-2-1`. Each consumer runs at 1/2 utilization. There is only one
producer of each item, so same-item reassignment cannot drop any of these four positive requirements.

`restrictedOrders` enumerates both source-block orders, both target-block orders and every
within-bank order of their two active endpoints: 64 cases, minimum one inversion. Assumptions are
contiguous facility terminal banks on two opposing faces and paths staying inside that corridor.
Unused port spacing does not change relative order. This proves neither global nonplanarity nor
infeasibility under another embedding. The fixture's single preplanned bridge realizes the bound
and the independent validator confirms complete local recipe coverage.

## Orthogonal bundle scope

`crossingBundle` preplans a rectangular matrix with one bridge per horizontal/vertical lane pair.
Items may repeat along one axis, but the two axes must be commodity-disjoint and on the same
transport layer. Each lane fits catalog line and bridge capacity. The core has M columns by N rows;
the reserved boundary rectangle adds one cell on each side. It is an orthogonal crossing primitive,
not arbitrary N:M allocation, branching, merging or a general lane permutation network.

`bundle-audit.mjs` validates 72 cases through the existing optimized executable: belt and pipe,
1..6 lanes per axis, up to 36 adjacent bridges. Witness v1 requires a facility, so an unconnected
catalog anchor sits outside the transport paths. Its missing recipe terminals remain explicit and
`recipe_coverage_complete` is false. This test checks physical boundary-to-boundary component
geometry, not actual facility port capacity for 6 pipe lanes. Six is a test range, not a game limit.

## Verification and next slice

Fifteen Node tests pass across bound-fixture and permutation modules. Six full-rate CLI controls
pass (`full-audit.json`); all 72 bundle cases pass. Workspace check and all 524 Rust tests passed,
with two existing ignores; this extension changes no Rust code. Actual full-rate, corrected and
two-supplier images were inspected; six viewport/theme checks pass per page.

Single observations: full-rate construction 7.27 ms, validation 0.743 ms; ordered transformation plus
construction 8.96 ms, validation 0.641 ms; two-supplier construction 6.48 ms, validation 0.570 ms.
Builds, browser work and tests are excluded. These are local diagnostics, not performance claims
about full-factory generation. Runtime catalog/witness hashes are retained in validation reports.

Next: a common JavaScript two-bank interface constructor taking recipe instances/utilizations,
without input coordinates, port IDs or bend points. It should allocate compatible flow, expand
capacity lanes, choose bank/port order, embed ordered approaches and plan residual crossings.
Keep unsupported topology and capacity/port failures explicit. Fixed fixtures become reference
examples, not source geometry. The user postponed Rust migration until efficacy is established.

Run `generate-dedicated.mjs` with the same catalog/output flags as `generate.mjs`, but no case or
external policy flags, selecting `full-fixture.json` or `two-supplier-fixture.json`. Run `reorder.mjs`
with those catalog/fixture flags plus new report/witness/detail output paths. `bundle-audit.mjs`
takes catalog flags, `--fixture full-fixture.json`, `--validator`, a new `--work-directory`,
`--report-output` and `--max-lanes 6`. All detailed output remains in JSON files.

Tailnet pages: `bound-interconnect-ordered.html` and `bound-interconnect-two-supplier.html` on the
existing port-8765 server. Earlier fixed-order images remain as labeled baseline evidence.
