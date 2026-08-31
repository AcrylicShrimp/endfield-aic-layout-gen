# Heavy Xiranite Port-Selected Element Reformulation Report

## Outcome

The endpoint selector was cut over from a scalar placement-port Cartesian index to an exact
port-selected variable-element encoding. The faithful five-port Powder input changed from no
incumbent in 5,000 ms to a first incumbent in 305 ms and a complete lexicographic optimum in
495 ms. Peak RSS fell from 61.03 MiB to 33.02 MiB in the isolated single-connector process.

The full four-requirement phase-zero model also improved. It changed from a five-second feasible
incumbent `(42 area, 12 transport tiles, 2 turns)` with no proof to a fully proven optimum
`(42 area, 4 transport tiles, 0 turns)` in 1,829 ms of search.

This result accepts the endpoint selector reformulation as the new exact baseline. It does not
establish that variable-element is optimal for every later production phase, but it removes the
measured phase-zero selector cliff without restricting the solution set.

## Exact Reformulation

Let `P` be the selected facility placement, `Q` the selected compatible port, and `G` the resulting
connection geometry. For `k` compatible ports, the previous v1 encoding used:

```text
C = P * k + Q
G = flattened_geometry[C]
```

`C` had `placement_count * port_count` values. The new v2 encoding uses one exact placement lookup
per port and lets `Q` select the resulting geometry variable:

```text
H[q] = geometry_for_port_q[P]  for every compatible port q
G = H[Q]
```

The two formulations are equivalent because `flattened_geometry` is the row-major concatenation
of the per-port placement lookup values. For every legal `(P, Q)` pair they produce the same `G`.
An unavailable port-placement pair still produces sentinel `-1`, which the non-negative `G` domain
rejects exactly as before.

Port identity remains explicit. Distinct external requirements still cannot select the same
facility port. Facility placement, rotation, compatible port, connector template, used bounds, and
all objective values remain solver decisions. No coordinate rule, port preference, candidate
restriction, warm-start fixation, or routing heuristic was added.

The exact formulation identifier was advanced from
`joint-shared-transport-layer-external-connectors-v1` to
`joint-shared-transport-layer-external-connectors-v2` so stored diagnostics identify the hard
cutover.

## Five-Second Port-Domain Comparison

Each case uses Heavy Xiranite phase zero route index 1, a caller-supplied 12 by 12 ceiling, an
isolated release process, and a 5,000 ms search budget. Restricted cases remain diagnostic-only;
`faithful-all` is the full legal five-port domain. Objective vectors are `(area, tiles, turns)`.

| Case | Ports | v1 result | v1 first | v1 search | v2 result | v2 first | v2 search | v2 objective | v2 RSS |
| --- | ---: | --- | ---: | ---: | --- | ---: | ---: | --- | ---: |
| `one-collinear` | 1 | optimal | 7 ms | 976 ms | optimal | 8 ms | 507 ms | (30, 1, 0) | 26.94 MiB |
| `two-collinear` | 2 | optimal | 1,137 ms | 1,198 ms | optimal | 381 ms | 449 ms | (30, 1, 0) | 28.33 MiB |
| `four-collinear` | 4 | optimal | 2,221 ms | 2,295 ms | optimal | 163 ms | 352 ms | (30, 1, 0) | 31.89 MiB |
| `one-orthogonal` | 1 | optimal | 10 ms | 103 ms | optimal | 10 ms | 172 ms | (30, 1, 0) | 25.64 MiB |
| `mixed-two` | 2 | optimal | 226 ms | 411 ms | optimal | 169 ms | 228 ms | (30, 1, 0) | 27.80 MiB |
| `faithful-all` | 5 | unknown | none | 5,000 ms | optimal | 305 ms | 495 ms | (30, 1, 0) | 33.02 MiB |

All v2 witnesses passed independent validation and all six v2 cases proved the complete
lexicographic optimum. The one-port orthogonal case regressed by 69 ms of search but remains a
172 ms proof. Every multi-port case improved, including the previously failing faithful case.

## Structural Comparison

The faithful single-connector model changed only slightly in raw size:

| Metric | v1 | v2 | Delta |
| --- | ---: | ---: | ---: |
| Endpoint variables | 3 | 7 | +4 |
| Total variables | 4,441 | 4,445 | +4 |
| Total constraints | 23,628 | 23,632 | +4 |
| Total terms | 81,729 | 81,741 | +12 |
| Search | 5,000 ms, unknown | 495 ms, optimal | cliff removed |
| Peak RSS | 61.03 MiB | 33.02 MiB | -45.9% |

The full phase-zero model adds three endpoint variables and three element constraints for each
external requirement according to its compatible port count. Relative to v1 it adds 12 variables,
12 constraints, and 36 terms, while reducing the search from an unproven five-second run to a
1,829 ms complete proof.

This confirms that raw variable count and log-domain volume were misleading for this cliff. The v2
model has slightly more integer variables and a larger recorded log-domain volume, but its direct
factorization propagates placement, port, and geometry decisions much more effectively.

## Full Phase-Zero Comparison

| Metric | v1 | v2 |
| --- | ---: | ---: |
| First incumbent | 47 ms | 78 ms |
| Search termination | feasible at 5,001 ms | optimal at 1,829 ms |
| Objective | (42, 12, 2) | (42, 4, 0) |
| Variables | 15,758 | 15,770 |
| Constraints | 70,510 | 70,522 |
| Terms | 201,101 | 201,137 |
| Peak RSS | 70.91 MiB | 75.12 MiB |
| Validation | passed | passed |

The isolated faithful connector's memory use improved substantially. Full phase-zero peak RSS rose
by 4.21 MiB, or 5.9%, while solve quality and proof time improved. RSS values are single-run
observations; structural counts are deterministic for this revision.

## What Became Simpler

- The model no longer invents a scalar ID whose only meaning is the Cartesian product of two
  already independent decisions.
- Placement remains one variable and port identity remains one variable instead of being hidden
  inside a combined index.
- Geometry is expressed as the composition of two ordinary element relations: lookup by placement,
  then selection by port.
- The v2 diagnostic directly reports per-port geometry variables rather than a 1,280-value
  placement-port auxiliary domain.

## Verification

- `cargo fmt --all`
- `cargo check --workspace`
- `cargo test --workspace`: 172 tests passed
- release-mode six-case port-domain matrix
- release-mode full four-requirement phase-zero solve
- independent witness validation for every successful artifact
- self-contained success HTML for every v2 case

## Artifacts

- v1 diagnosis:
  `docs/benchmarks/heavy-xiranite-minimum-rate.fourteenth-external-connector-port-domain-cliff-report.md`
- v1 normalized baseline:
  `docs/benchmarks/heavy-xiranite-external-connector-port-domains/summary.json`
- v2 normalized comparison:
  `docs/benchmarks/heavy-xiranite-external-connector-port-elements/summary.json`
- v2 per-case JSON, HTML, and `/usr/bin/time -l` records:
  `docs/benchmarks/heavy-xiranite-external-connector-port-elements/`

## Next Research Boundary

The placement-port selector is no longer the current phase-zero blocker. The next cliff diagnosis
should return to cumulative SCC growth and identify the first enlarged production phase that fails
to find or improve an incumbent under a controlled budget. Do not assume the same reformulation is
the dominant improvement in phases containing internal shared transport networks; measure the next
cliff before selecting another target.
