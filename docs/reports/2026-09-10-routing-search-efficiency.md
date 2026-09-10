# Routing search efficiency: shared work exhaustion masks useful retries

The local routing engine rejects a substantial fraction of PPO edits because its
first routing choice consumes the shared work allowance. A frozen sample recovers
9 of 17 budget failures at a larger allowance without changing the action, factory
rules or repair limits. Only two recovered edits immediately improve best area.
The measured problem is lost valid edits and wasted search, rather than evidence
that these factory movements are inherently impossible.

This investigation leaves the twelve-hour training protocol and production router
unchanged. It uses committed records, frozen pre-edit states and an instrumented
native library built in a separate checkout. Source baseline: `b99b86a`.

## Frequency and coverage

Freeze the run at 444 new training batches (476 including its 32-batch ancestor).
The latest completed evaluation is new batch 440 (cumulative 472).

| Population | Completed actions | Executor rejected | Work-budget failures |
| --- | ---: | ---: | ---: |
| All 444 training batches | 445,952 | 146,847 (32.93%) | Full cause coverage unavailable |
| All 56 evaluation phases | 57,344 | 18,481 (32.23%) | 8,848 (15.43% of actions; 47.88% of rejections) |
| Latest evaluation, 32 factories | 1,024 | 380 (37.11%) | 207 (20.21% of actions; 54.47% of rejections) |

Training totals cover 13,936 complete episodes of 14,208 attempts. Retained detail
covers only 283 episodes / 9,056 actions, with 2,403 rejections and 990 work-budget
failures. Retention selects particular batches; this is not a representative
sample from which to estimate the full training failure-cause distribution.
All 1,792 evaluation episodes retain detail, but reuse the same 32 factories
across policies. They are repeated observations, not independent factory samples.

A budget failure is counted once per action when either `local-work-budget` or
`region-work-exhausted` appears. Nested diagnostic codes can overlap and must not
be added together. In the latest evaluation, the 380 rejected actions comprise
207 budget failures, 167 `local-no-path` and six port-assignment failures. Of the
686 regional repair calls, 255 produce candidates, 51 are unchanged, 207 exhaust
work, 106 exhaust attempts and 67 terminate at a fixed boundary. These local
termination reasons do not prove global infeasibility.

Of the 207 budget failures, 134 (64.73%) stop during their first repair attempt;
52 stop during the second, 12 during the third and nine during the fourth.
Facility-pose edits fail 362 of 562 times (64.41%). Therefore total action rejection
rates, which include cheap restore and other edits, understate the difficulty
encountered by facility movement specifically.

Budget failures consume 44,240,000 of 56,398,575 evaluation expansions (78.44%).
For the latest cohort the corresponding share is 1,035,000 / 1,243,430 (83.24%).
Their measured executor time is 0.523 s of 0.888 s in that cohort; all episode
work sums to 12.406 s. Over all evaluations, those times are 22.484 / 41.865 /
771.365 s respectively. These are summed worker instrumentation, not wall-clock
training critical-path measurements. Routing rejection is common, but this data
does not establish routing as the dominant end-to-end training time cost.

## Controlled budget replay

Before varying work, select the first budget-failed action in each affected
factory in the latest cohort: 17 factories out of 32. Replay each original prefix,
match recorded action/status/current/best identities and expansion counts, then
freeze the exact pre-edit layout. Check the isolated baseline again. Every found
candidate is independently admitted against the original production request.

Default repair limits remain four attempts, 64 chains, eight extra chains, four
port trials and eight crossings per route. The only changed input is total work.
Each table entry includes all 17 actions, including actions already recovered at
a lower allowance; each trial starts from the same pre-edit state.

| Work allowance | Valid candidates | Improve preceding best area | Actual expansions, sum | Executor ms, sum |
| ---: | ---: | ---: | ---: | ---: |
| 5,000 | 0 / 17 | 0 | 85,000 | 37.66 |
| 10,000 | 4 / 17 | 1 | 153,954 | 57.56 |
| 25,000 | 7 / 17 | 2 | 297,830 | 100.42 |
| 50,000 | 9 / 17 | 2 | 377,655 | 124.61 |

Times come from the original matrix during concurrent training, not an isolated
latency benchmark. A second run with the retained CLI reproduces all 68 trials'
non-time results exactly. Recovered areas include 1,596 -> 1,444 and
1,974 -> 1,428. The other recovered edits provide valid exploration states with
unchanged area, not demonstrated quality improvements. Eight failures remain at
50k; six hit other local stopping conditions and two still exhaust work.
Neither the selected failure sample nor one-step rescues predicts the learning
benefit of globally raising budgets.

## Concrete cause in the recorded facility 15/16 move

The earlier [recorded-move probe](2026-09-10-recorded-pose-routing-probe.md)
proves the movement is routable. Capture all path inputs for its 5k and 10k
replays; the diagnostic build reproduces status, action, witness hashes and total
expansions. Independently replicate the native A* frontier ordering and state
costs: all ten path calls match native statuses and expansion counts exactly.
The replica checks search behavior, not reconstructed witness validity; the
native validator checks the completed factory.

At attempt zero, chains 83 and 52 route in 25 and 216 expansions. Chain 58 then
tries (35,25) -> (36,8), with an eastward first step and its current input port.
The frozen bounds are 41 x 34. A* spends 8,531 expansions and returns local
`no-path`: 551 physical cells become 1,493 oriented cells and 8,531 expanded
states once crossing-count labels are included. At 5k, the call receives only
4,759 remaining expansions and terminates before learning this.

A reverse reachability relaxation retains entry/exit direction and straight
crossing rules while dropping the crossing-count limit and allowing repeated
cells. Even this larger path space cannot reach the start from the goal. Only
37 oriented states on ten cells can reach the goal in the relaxed graph. Thus
this *particular frozen path query* is unreachable; it does not require a
forward flood across thousands of crossing-count states to discover that.
The diagnostic currently materializes a small graph to establish this fact;
37 is reverse-reachable state count, not measured production preprocessing cost.

Attempt one changes facility 16 from `input-belt-0` to `input-belt-1` and moves
chain 58 first in the order. All four chains finish in 113 + 211 + 192 + 53 =
569 expansions, with no additional repair chains. The total is 9,341, of which
8,531 (91.33%) went into the failed fixed choice. Port and order change together;
their individual effects remain unisolated. This move preserves area 1,394 and
increases secondary cost, so it is a valid exploration witness.

## Implementation mechanisms and follow-ups

Confirmed directly in the native source:

- `full_factory/path.rs`: A* state is `(cell, direction, crossing_count)`.
  Manhattan distance times the length weight is the heuristic. It cannot see
  obstacles, one-way endpoint access or disconnected regions. There is no
  directed reachability precheck and no cross-count label dominance.
- `full_factory/routing.rs`: affected chains are routed greedily, pipes first,
  then the prioritized chain, longer endpoint distance and chain ID. Completed
  paths reserve occupancy for later paths. Each call receives all remaining
  transaction work, so one bad query can consume the entire allowance.
- `full_factory/routing/region.rs`: alternatives are discovered after failure,
  but a fully consumed transaction budget ends repair before those alternatives
  can run. Retry can change port/order and expand bounded ownership; these are
  available mechanisms whose opportunity is constrained by the first search.

The next focused experiment should compare the frozen sample under the same
5k allowance with a directed reachability precheck and explicit work accounting.
A negative relaxed-reachability result can safely reject only its fixed query;
positive reachability still requires the real crossing-budget path search.
Preserve blocker diagnostics so early failure still enables the existing repair
choices. Reverse obstacle-aware distance fields may also improve A* guidance,
but their construction cost must be included and measured.

A separate experiment should reserve work for alternative ports/orders or use
progressive per-query allowances, with cutoff distinguished from proven local
no-path. This changes scheduling and can harm routes that need a longer first
search, so compare candidate yield, score and total work across all selected
cases, rather than only the motivating example. Cross-count dominance is another
candidate, but requires proof under route-history/repeated-cell semantics before
being treated as a sound optimization. None of these changes is implemented in
the production router by this investigation.

## Reproduction and retained evidence

Run census and matrix CLIs from the frozen runtime
`/home/ashrimp/Srv/aic-ppo-cnn2-12h-20260910`; copy the new diagnostic scripts
there without replacing training sources. Use fresh output directories:

```sh
PY=tools/factory-edit-pilot/.venv/bin/python
RUN=artifacts/factory-edit-pilot/cnn2-twelve-hour
EVIDENCE=artifacts/factory-edit-pilot/routing-census-batch-444
$PY tools/factory-edit-pilot/census_ppo_routing.py --run "$RUN" \
  --snapshot "$EVIDENCE/snapshot.json" --output "$EVIDENCE/census-replay"
$PY tools/factory-edit-pilot/probe_ppo_routing_matrix.py --run "$RUN" \
  --sample "$EVIDENCE/sample.json" --output "$EVIDENCE/matrix-replay"
```

For a new live snapshot, replace `--snapshot` with
`--job current-history-16--seed-17`. Detail coverage reflects trajectories present
at read time and may shrink after cleanup even when committed record totals stay
fixed. Both retained census runs matched completely for this investigation.

For path capture, use an isolated checkout of `b99b86a`, copy the diagnostic CLI
and `routing_path_capture.patch` into it, apply that patch with `git apply`, and
build `aic-pilot-native` in its own target directory with `--release -j 2
--features extension-module` and the frozen Python environment. Do not apply the
patch or rebuild the native library inside an active training runtime. Then run:

```sh
$PY tools/factory-edit-pilot/capture_ppo_routing_paths.py \
  --run /home/ashrimp/Srv/aic-ppo-cnn2-12h-20260910/artifacts/factory-edit-pilot/cnn2-twelve-hour \
  --probe /home/ashrimp/Srv/aic-ppo-cnn2-12h-20260910/artifacts/factory-edit-pilot/pose-15-16-step31-verified \
  --native-library target/release/libaic_pilot_native.so \
  --output artifacts/route-trace-replay
$PY tools/factory-edit-pilot/analyze_routing_path_trace.py \
  artifacts/route-trace-replay/work-*/path-*.json
```

The dump patch is retained experiment source, not a production hook or generated
trace. Captured grids, action logs and witnesses remain ignored under `artifacts/`.
The adjacent compact provenance manifest binds reports and capture inputs. Raw
original matrix and selected witnesses are retained on the server under
`routing-census-batch-444/work-matrix`; verified CLI results are in
`matrix-verified`. Instrumentation lives separately at
`/home/ashrimp/Srv/aic-routing-diagnosis-20260910/artifacts/route-trace` and
`route-trace-verified`. Local aggregate/capture copies are under
`artifacts/factory-edit-pilot/routing-census-batch-444`.

Validation: all 68 budget trials replayed twice; all ten path searches matched
native status/count; instrumented actions matched originals; census reproduced
exactly; Python compilation, workspace checks/tests and artifact-policy checks
passed. Production routing and training behavior were not modified.
