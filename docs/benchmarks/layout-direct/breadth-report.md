# Broader-load streaming run: disconnected-component admission gap

Status: implementation limitation verified; construction policy decision required. Not ready,
not a timeout and not a full-factory infeasibility proof.

## Actual main-path result

The sampler now requests exact target loads 1/4, 1/2, 1, 2, 4 and 8 from runtime recipes.
Loads above one request multiple legal facility instances; they do not overutilize one machine.
The training curriculum explicitly requests small, medium and large roots in that order,
repeating. Off-stratum requests are recorded before geometry, not silently replaced. Complete
factory validity and observation grid/token support are now separate fields.

The command starts real training on one newly generated seven-facility mixed-transport factory
with an active two-node cycle. Four 512-step trajectories execute 2,048 actions; all four
retained outputs pass fresh independent geometry and original coverage audits. The optimizer
update and checkpoint are saved. The next required medium slot exhausts 200 sampled requests,
and the driver exits 1 with `streaming-training-failed`, explicitly refusing an easier-stratum
or saved-root fallback. No large slot or evaluation is reached. This is not a completed model run.

Across all 203 attempts: 105 exact-flow rejections, 83 off-stratum requests, one accepted factory,
13 construction stops and one duplicate. The actual solved-size counts are 70 small, 16 single,
12 medium and 105 unknown because flow was rejected. Eleven distinct medium requests are sent
to construction; the twelfth is a duplicate. The other two construction failures occur in the
initial small slot. There are no construction timeouts or observation-limit rejections.

## Verified cause

Every stopped construction completes exactly the internal weak component containing its seed.
No unresolved internal requirement remains incident to a placed facility, but other required
facilities are still missing. At the terminal failure there are zero candidate groups, zero
new placement attempts and zero A* searches. Earlier successful steps can have ordinary local
rejections; those are not the cause of the final empty frontier.

Representative original requests:

| Ordinal | Required facilities | Internal component sizes | Retained facilities |
| --- | --- | --- | --- |
| 0 | 4 | 1 + 1 + 1 + 1 | 1 |
| 6 | 20 | 5 + 5 + 5 + 5 | 5 |
| 40 | 32 | eight groups of 4 | 4 |
| 83 | 36 | 9 + 9 + 9 + 9 | 9 |
| 127 | 20 | 10 + 10 | 10 |
| 180 | 24 | 12 + 12 | 12 |

The pure `inspectFrontier` audit independently reconstructs seed connectivity from original
wiring and checks the actual final report; it confirms this condition for all 13 stops. Source
inspection matches it: `constructive/production.rs` builds its frontier only from unresolved
facility-internal edges incident to already placed members. Parallel components may share
declared external/target nodes but have no such internal edge. More local search budget cannot
create a candidate in this unchanged frontier algorithm.

This is an implementation-level explanation. It does not prove that independent production
branches cannot coexist in a valid factory. Do not delete those branches, lower the requested
rate, add external intermediates, fake internal edges, or count single-component substitutes as
the required medium/large examples.

## Policy boundary and next decision

The current AGENTS/design rule requires one connected growth subgraph. Supporting disconnected
internal production components needs an explicit construction contract, not a silent sampler
workaround. `parallel-components-decision.md` proposes component-level admission while preserving
connected, immediately routed facility growth inside every component, original full-request
rates and IDs, global external-boundary completion and independent combined-factory validation.
It does not freeze components as immutable macros for improvement.

That proposal is not implemented or treated as approved. The measured run stops here pending
the user's construction-policy choice. The larger learned-model goal remains active and unmet;
there is no goal-level impossibility verdict.

## Evidence and verification

`breadth-results.json` records all 13 original wiring/report hashes and no-frontier checks.
`target/layout-direct-breadth-01/worker-0/generation-report.json` preserves every attempt and
source plan. `group-0001.json` and `checkpoint-0001.json` preserve completed training; the absent
final `report.json` must not be replaced with a fake successful training report.

```sh
uv run --frozen --project docs/benchmarks/layout-learning python docs/benchmarks/layout-direct/train.py \
  --plan docs/benchmarks/layout-direct/sampling.json --config docs/benchmarks/layout-direct/breadth.json \
  --output-directory target/layout-direct-breadth-01
node docs/benchmarks/layout-direct/breadth-analysis.mjs \
  --directory target/layout-direct-breadth-01 --output docs/benchmarks/layout-direct/breadth-results.json
node --test docs/benchmarks/layout-direct/*.test.mjs
uv run --frozen --project docs/benchmarks/layout-learning python -m unittest discover \
  -s docs/benchmarks/layout-direct -p 'test_*.py'
cargo check --workspace
cargo test --workspace
```

Use fresh output paths. Fifteen direct Node tests and nine Python tests pass, including scaled
target demand without changed sources, schema migration diagnostics, exact observed grid/token
limits without cropping, catalog-sized components, solved SCC/weak-component distinction and
no-frontier versus routing failure. Runtime hashes pass the worker shutdown check. A post-run
configuration-type guard is additionally covered by tests. Workspace check/tests pass; no Rust
source changes. The accepted connected constructor and production commands are unchanged.

Sampling schema 1 is explicitly rejected in favor of schema 2 `target_loads`. Training schema
3 requires explicit train/evaluation size schedules; `any` is the unstratified control. Historical
small-load configurations and results remain at commits `8d317f5` (first run) and `f700770`
(credit run), rather
than behind a runtime compatibility adapter. What is simpler is the distinction between target
production load, individual machine utilization, construction success and learner support.
