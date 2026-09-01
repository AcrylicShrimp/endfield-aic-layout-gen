# Heavy Xiranite Endpoint Clearance Execution Yield

## Result

The Phase 32 endpoint-clearance cliff is dominated by relation executions that make no domain
change. Across Phases 29, 31, and 32, approximately 99.91% of custom propagator executions reject
no orientation, tighten no coordinate bound, and report no conflict. Approximately 97-99% of all
executions are scheduled only by coordinate-bound events.

The notification fan-out is bipartite. One endpoint coordinate change notifies every relation from
that endpoint to a non-owning facility. One facility coordinate change notifies every relation
from that facility to a non-owned endpoint. At Phase 32, target-facility coordinate notifications
account for 62.4% of all notifications and endpoint connection-coordinate notifications account
for 31.8%.

This makes duplicated relation scheduling an exact reformulation candidate. It does not prove that
scheduler fan-out causes the search cliff: the no-effect ratio is almost identical in the easy
Phase 29 control, and the generic clearance formulation also times out with fewer solver
propagations. Search growth can cause more clearance work just as clearance overhead can reduce
search throughput. No individual relation may be dropped and no coordinate may be restricted.

## Instrumentation

Rung schema 9 classifies each propagated clearance execution by the event families coalesced before
that execution:

- initial or otherwise unclassified scheduling;
- endpoint/facility coordinate events only;
- orientation events only;
- both coordinate and orientation events;
- explicit scratch propagation when Pumpkin invokes that callback.

It also records whether an execution rejects an orientation, detects a unique separation, tightens
a coordinate bound, or reports a conflict. `executions_without_domain_effect` means no orientation
rejection, bound update, or conflict. A repeated unique-separation detection with already-tight
bounds is correctly counted as having no domain effect.

The pending event mask is diagnostic-only and is cleared on solver synchronisation. It does not
change event registration, enqueue decisions, propagation, reasons, branch order, or accepted
assignments.

## Measured Yield

Each row is one fresh release process with High clearance priority, the default false-event filter
disabled, a caller-supplied 50x50 ceiling, and a five-second budget. Phase 29 finishes before the
budget. Timings are not compared with schema 8 because the additional relaxed atomic counters add
diagnostic work; solver decisions and semantic outcomes remain the relevant controls.

| Phase | Facilities | Endpoints | Outcome | Executions | Coordinate-only | No domain effect | Bound-update executions | Conflict executions |
|---:|---:|---:|---|---:|---:|---:|---:|---:|
| 29 | 30 | 95 | feasible | 1,543,790 | 1,517,739 (98.31%) | 1,542,540 (99.919%) | 1,210 (0.0784%) | 39 (0.0025%) |
| 31 | 36 | 115 | unknown | 33,932,252 | 33,480,125 (98.67%) | 33,901,305 (99.909%) | 29,760 (0.0877%) | 1,133 (0.0033%) |
| 32 | 39 | 125 | unknown | 34,963,516 | 34,043,712 (97.37%) | 34,932,392 (99.911%) | 29,804 (0.0852%) | 1,165 (0.0033%) |

Run-to-run throughput changes with machine timing, so the absolute Phase 32 execution count differs
from the earlier 39.15-million run. The yield ratio is the result: only about 0.089% of executions
produce a rejection, bound update, or conflict.

## Phase 32 Notification Axis Breakdown

One additional Phase 32 run separates the four coordinate registrations.

| Notification source | Count | Share of all notifications |
|---|---:|---:|
| endpoint connection x | 7,249,640 | 16.38% |
| endpoint connection y | 6,807,738 | 15.38% |
| target facility x | 14,271,048 | 32.24% |
| target facility y | 13,339,459 | 30.13% |
| target orientation | 2,599,384 | 5.87% |
| total | 44,267,269 | 100% |

The current model posts 4,750 independent endpoint/non-owner-facility propagators. Facility
coordinates have the larger fan-out because one facility move is delivered independently to
almost every endpoint relation involving that facility. Endpoint coordinates similarly fan out to
every non-owner facility.

## Entailment And Hot-Set Census

At any search node, a relation is universally entailed when every surviving target orientation is
already guaranteed to put the endpoint outside the target rectangle for every value remaining in
the four coordinate domains. Domain shrinking preserves that fact until backtracking.

| Phase | Universally entailed executions | Share of executions | Later notifications in an entailed episode | Share of notifications |
|---:|---:|---:|---:|---:|
| 29 | 641,929 | 41.58% | 486,111 | 25.47% |
| 31 | 9,131,907 | 26.28% | 6,988,531 | 16.08% |
| 32 | 7,151,424 | 20.66% | 5,658,505 | 12.93% |

Exact subtree-local dormancy could avoid some enqueue requests or executions after these callbacks,
but `notifications_while_entailed` is only an opportunity ceiling because scheduler coalescing is
not one-to-one. Its Phase 32 ceiling is modest relative to the entire cliff. It remains a valid
local optimization experiment, not a complete explanation.

The Phase 32 relation distribution is diffuse:

Percentiles use the nearest-rank-below convention over the zero-based sorted relation list:
`index = floor((relation_count - 1) * percent / 100)`.

| Metric | Value |
|---|---:|
| collected relations | 4,750 |
| zero-execution relations | 0 |
| median executions per relation | 7,781 |
| p95 executions per relation | 14,574 |
| maximum executions for one relation | 18,267 |
| hottest one relation share | 0.0523% |
| hottest ten relations share | 0.5123% |
| hottest one hundred relations share | 4.7214% |
| hottest target facility share | 4.6431% |
| hottest endpoint share | 1.3730% |

There is no small data-specific hot set to remove or special-case. The cost is spread across the
complete exact relation family, supporting a structural batching experiment rather than a
frequency-based restriction.

## Expanded Counter Cost Control

Four Phase 32 five-second runs with expanded counters enabled reach a median 211,004 decisions and
about 46.36 million solver propagations. Four counter-disabled runs reach a median 212,533
decisions and about 46.64 million propagations. The median difference is below 1%, while the
counter-disabled samples have a much wider run-to-run range. This control does not resolve a stable
counter cost; it shows that the 99.91% yield and diffuse-hot-set result are not artifacts of one
exceptionally slow enabled run.

At Phase 29, four enabled and four disabled runs follow the identical 13,332-decision,
2,178,513-propagation trajectory. Median times are 223.5 and 229.5 milliseconds respectively; the
counter-enabled sample happens to be faster, which is consistent with short-run wall-time noise
rather than evidence of a counter speedup.

## Exact Reformulation Candidate

The smallest batching A/B is one indexed propagator per target facility: 39 shards containing the
same 4,750 point-versus-rotated-rectangle relations. This targets the measured facility-coordinate
fan-out without immediately introducing one global high-priority propagator.

1. each shard registers its target facility coordinates and orientations once;
2. it registers every non-owner endpoint coordinate in that shard once;
3. endpoint events dirty only that endpoint relation, while target coordinate or orientation events
   dirty every relation in the shard;
4. a relation is evaluated once per dirty batch, but any later relevant domain event must dirty and
   re-evaluate it again, including within the larger solver fixpoint;
5. scratch propagation evaluates every relation regardless of transient dirty state;
6. the current eager relation-specific rejection, coordinate-bound, conflict, and proof reason are
   reused unchanged;
7. synchronisation discards all transient dirty state and non-trailed caches so backtracking cannot
   suppress a future check.

This changes neither the feasible set nor the decision variables exposed by the contract, and it
does not pre-fix any decision. It changes event registration and the batching of the same exact
inferences, so the observed decision, conflict, and learning trajectory may change. It should be
introduced as an independent rung for A/B comparison before replacing the
relation-per-propagator baseline.

The primary measurements are propagator instances, watcher registrations by role, notification
callbacks, enqueue requests, shard invocations, full-shard and endpoint-only batches, dirty relation
checks, maximum and mean dirty batch size, relation-level domain-effect yield, branch decisions,
conflicts, solver propagations, and first-witness time at Phases 29, 31, and 32. Grouped invocation
counts must not be compared directly with pairwise relation execution counts.

## Independent Review Resolution

Three reviewers independently examined proof safety, implementation architecture, and alternative
exact strategies.

- The soundness review **blocked the original global wording**, not the batching concept. It required
  re-dirtying after later events, complete scratch propagation, synchronisation cleanup, unchanged
  eager reasons, complete adjacency indexes, differential tests, and a backtracking regression.
  Those requirements are now the candidate contract above; no grouped implementation is included
  in this diagnostic slice.
- The implementation review recommended target-facility shards before a global propagator. It
  confirmed the 4,750 relation construction and estimated that facility and orientation watcher
  requests can be reduced substantially, while warning that relation evaluations may remain.
- The alternative-strategy review accepted clearance as the first difficult semantic block but
  blocked a causal claim from no-effect ratios alone. Its requested trigger/effect cross-tab,
  entailment census, relation/endpoint/target hot set, and expanded counter control were added. A
  fixed-decision CPU profile remains a grouped A/B measurement requirement rather than evidence
  claimed by this report.

No reviewer approved a heuristic restriction or a game-rule change.

## Artifacts

- `heavy-xiranite-bottom-up-clearance-execution-yield/phase29/`
- `heavy-xiranite-bottom-up-clearance-execution-yield/phase31/`
- `heavy-xiranite-bottom-up-clearance-execution-yield/phase32/`
- `heavy-xiranite-bottom-up-clearance-execution-yield/phase32-axis-breakdown/`
- `heavy-xiranite-bottom-up-clearance-execution-yield/phase{29,31,32}-entailed/`
- `heavy-xiranite-bottom-up-clearance-execution-yield/phase32-hotset/`
- `heavy-xiranite-bottom-up-clearance-execution-yield/counter-{on,off}-phase{29,32}-run{1,2,3,4}/`
