# Heavy Xiranite External Connector Port-Domain Cliff Report

## Result

The next exact-solver improvement target is the endpoint selector's placement-port geometry
channel. The current factored endpoint model still creates one Cartesian integer index for
`placement choice x port choice`, then maps that index through a constant-element constraint to a
geometry key. This exact encoding preserves semantics, but the phase-zero Powder input exposes a
severe many-to-one search and propagation cliff in that channel.

The target is not a port restriction, port preference, connector-template heuristic, or solver
architecture change. The replacement must preserve every placement, rotation, compatible port,
connector template, and objective value. It should channel the independent placement and port
variables to geometry without the scalar Cartesian index, for example through exact per-port
placement lookups guarded by the port selector. Candidate exact encodings must be benchmarked
before one is adopted.

## Controlled Experiment

All six cases select only route index 1, the Xiranite Powder belt input in Heavy Xiranite phase
zero. Every case uses the unchanged
`joint-shared-transport-layer-external-connectors-v1` formulation, a caller-supplied 12 by 12 hard
ceiling, a 5,000 ms release-mode search budget, and a separate process measured with macOS
`/usr/bin/time -l`.

Facility placement, facility rotation, one retained compatible port, one of three connector
templates, used geometry, and every objective stage remain solver decisions. Five restricted
cases are explicitly `diagnostic-only`; only `faithful-all` preserves the production problem. The
CLI rejects a restricted domain mislabeled as faithful and a full domain mislabeled as
diagnostic-only.

## Port-Domain Matrix

`Cartesian domain` is the domain size of the auxiliary placement-port index. Objective vectors are
`(used area, physical transport tiles, route turns)`. Timing and RSS are single-run observations;
structural model counts are deterministic for this revision.

| Case | Retained ports | Geometry class | Status | First incumbent | Objective | Cartesian domain | Variables | Constraints | Terms | Peak RSS |
| --- | ---: | --- | --- | ---: | --- | ---: | ---: | ---: | ---: | ---: |
| `one-collinear` | 1 | South | optimal | 7 ms | (30, 1, 0) | 256 | 3,383 | 19,176 | 70,431 | 27.89 MiB |
| `two-collinear` | 2 | South | optimal | 1,137 ms | (30, 1, 0) | 512 | 3,736 | 20,660 | 74,197 | 32.05 MiB |
| `four-collinear` | 4 | South | optimal | 2,221 ms | (30, 1, 0) | 1,024 | 4,440 | 23,628 | 81,729 | 43.73 MiB |
| `one-orthogonal` | 1 | East | optimal | 10 ms | (30, 1, 0) | 256 | 3,387 | 19,176 | 70,431 | 24.25 MiB |
| `mixed-two` | 2 | South + east | optimal | 226 ms | (30, 1, 0) | 512 | 4,088 | 22,144 | 77,963 | 29.08 MiB |
| `faithful-all` | 5 | Four south + east | unknown | none in 5,000 ms | none | 1,280 | 4,441 | 23,628 | 81,729 | 61.03 MiB |

Every successful case independently validated and proved the complete lexicographic optimum. The
faithful case returned `unknown`, not infeasible, and its HTML contains structured no-incumbent
evidence.

## The Critical Delta

The decisive comparison is `four-collinear` to `faithful-all`:

| Metric | Four collinear | Faithful five | Delta |
| --- | ---: | ---: | ---: |
| Search result | optimal in 2,295 ms | no incumbent in 5,000 ms | cliff |
| Placement-port domain | 1,024 | 1,280 | +256 values |
| Total variables | 4,440 | 4,441 | +1 |
| Log2 domain volume | 4,484.86 | 4,486.50 | +1.64 bits |
| Connector variables | 4,027 | 4,028 | +1 |
| Total constraints | 23,628 | 23,628 | 0 |
| Total terms | 81,729 | 81,729 | 0 |
| Peak RSS | 43.73 MiB | 61.03 MiB | +17.30 MiB |

Adding the east-side port does not add a new connector-option constraint or term after facility
rotation is included. The set of reachable physical geometry keys used by connector-template and
cell-occupancy expansion is already saturated by the four south-side ports. The one additional
connector variable is the fifth port-identity literal used by distinct-port accounting, not a new
ray geometry.

However, the endpoint selector still expands from 256 placements times four ports to 256
placements times five ports. The model defines this scalar product at
`shared_layer.rs:853-895`: it allocates `combined_choice`, posts
`combined_choice = placement_choice * port_count + port_choice`, and applies one constant-element
lookup. The fifth port therefore adds 256 extra selector states that map back into an already
represented physical geometry set. Port identity remains semantically important for multiple
requirements, but this particular channel makes equivalent geometry aliases expensive to search.

The connector builder then operates on the deduplicated reachable geometry keys at
`external_connectors.rs:193-242`. This explains why the connector constraints and terms are
identical between four and five ports while endpoint search behavior changes dramatically.

## What The Matrix Rules Out

- A single connector template plus dynamic used bound is not sufficient to cause the cliff. Both
  one-port cases prove optimality, and the east-side case finishes in 103 ms of search.
- Different unrotated facility sides are not sufficient. `mixed-two` proves the full optimum in
  411 ms.
- Same-side multiplicity alone is not sufficient. All four south-side ports prove the full optimum
  in 2,295 ms.
- Raw connector expansion is not the critical four-to-five delta. Constraints and terms do not
  increase at all.
- Commodity routing, flow conservation, splitters, convergers, bridges, crossings, and internal
  transport layers cannot be involved. This isolated phase-zero case contains none of them.
- Model construction is not the blocker. It remains 18-24 ms across all six cases.
- The 12 by 12 ceiling is controlled input capacity, not blueprint geometry; every successful
  witness uses a 5 by 6 bounding box.

## Final Improvement Target

Replace the endpoint selector's scalar Cartesian index and constant-element mapping with a stronger
exact channel between these three independent meanings:

1. selected facility placement,
2. selected compatible port identity, and
3. resulting connection geometry key.

The first implementation experiment should compare exact alternatives such as port-guarded
placement-to-geometry element constraints or a direct exact relation over placement, port, and
geometry. It must retain the explicit port ID because multiple requirements may compete for the
same physical port. It must not canonicalize away a port merely because this one-connector
diagnostic gives it duplicate geometry.

Acceptance for that experiment:

- all five legal Powder input ports remain selectable;
- the set of valid layouts and lexicographic objective values is unchanged;
- no coordinate, rotation, port, or connector template is chosen outside the solver;
- the six-case matrix still validates the same optimum for every restricted witness;
- `faithful-all` obtains a first incumbent and preferably proves area within the same five-second
  budget; and
- the full four-requirement phase-zero benchmark is rerun to detect proof-time regressions.

This is a semantics-preserving exact reformulation target. It is narrower than changing the whole
external connector model and directly attacks the only structural state that grows across the
observed four-to-five cliff.

## Cumulative Improvements Before This Target

| Checkpoint | Improvement | Measured outcome at 12 by 12 phase zero |
| --- | --- | --- |
| Release-mode baseline | Removed debug-build distortion | All solver performance claims use optimized Rust builds |
| Circulation permitted | Removed route-order proof state | Removed 432 variables, 1,584 constraints, and 4,752 terms |
| Shared transport layers | Replaced one dense grid per commodity with one belt layer and one pipe layer | Variables -24.0%; constraints -22.0% |
| Factored endpoints | Separated placement and port state | Endpoint states -66.9%; placement-routing incidences -43.2%; isolated RSS -10.2% |
| External boundary connectors | Replaced free external routing with three exact solver-selected straight templates | First valid full-set phase-zero incumbent improved from none in 15 seconds to about 50 ms |
| External subset diagnosis | Rebuilt all 15 external requirement subsets in isolation | Separated the single-Powder first-feasible cliff from the full-set area-proof cliff |
| Port-domain diagnosis | Varied only the compatible port domain of one Powder input | Located the remaining local cliff in the placement-port-to-geometry channel |

All cumulative formulation changes preserve solver authority over placement, rotation, legal port
choice, and the approved external connector template. No hand-written layout or routing heuristic
was introduced.

## Artifacts And Reproduction

- Contract: `docs/designs/external-connector-port-domain-cliff-diagnosis.md`
- Normalized matrix:
  `docs/benchmarks/heavy-xiranite-external-connector-port-domains/summary.json`
- Per-case JSON, self-contained HTML, and `/usr/bin/time -l` records:
  `docs/benchmarks/heavy-xiranite-external-connector-port-domains/`

The faithful case can be reproduced with:

```bash
/usr/bin/time -l -o faithful-all.time.txt \
  target/release/aic-cli research solve-first-phase-external-port-domain \
  --workload data/benchmarks/heavy-xiranite-minimum-rate.workload.json \
  --workspace-root . \
  --placement-request data/benchmarks/requests/placement.12x12.request.json \
  --case-id faithful-all \
  --classification faithful-baseline \
  --route-index 1 \
  --port-id input-belt-0 \
  --port-id input-belt-1 \
  --port-id input-belt-2 \
  --port-id input-belt-3 \
  --port-id input-belt-4 \
  --time-limit-ms 5000 \
  --output faithful-all.json \
  --visualization-output faithful-all.html
```

## Stopping Point

This checkpoint identifies the exact reformulation target and does not implement it. Review and
approval are required before replacing the endpoint selector encoding.
