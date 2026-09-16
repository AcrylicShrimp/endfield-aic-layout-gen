# Aggregate recipe-flow CP prototype

Date: 2026-09-16

## Decision question

The recipe-sized dataset generator needs to choose a target item, recipe
combination, external-input boundary and rate that realize a requested facility
count. A first sparse subtree DP attempt made cycle context part of its cache key.
On the 305-recipe game catalog it exhausted 200,000 states without matching a
16-facility request, so that implementation was discarded.

This probe asks whether a global constraint model is a better semantic-selection
kernel. It is deliberately separate from production generation and does not
claim compatibility with the existing path-specific source-plan contract.

## Prototype model

The research binary is `aic-recipe-cp-probe`. It uses a 20-second integral time
horizon, which is divisible by every current recipe duration. For each recipe
`r` it creates:

- integer facility count `x_r`;
- integer executions per horizon `q_r`; and
- binary activity `a_r`.

The capacity constraints make the facility count exact:

```text
q_r <= capacity_r * x_r
q_r >= capacity_r * (x_r - 1) + 1 when x_r > 0
sum(x_r) = requested facility count
```

Per-item balance includes recipe inputs and outputs, external input, surplus
waste and target output. An item cannot simultaneously be imported and exported
as waste. Target quantity can be fixed, left free inside the bounded horizon, or
maximized for diagnosis.

Three relevance formulations were compared:

1. no connectivity constraint;
2. one virtual-flow variable for each of 2,118 recipe dependency arcs; and
3. virtual flow on the 81-node SCC condensation graph, which has 132 arcs.

The probe also implements lazy active-graph cuts. It solves the compact balance
model, finds active recipes with no active dependency path to a target producer,
forbids that disconnected active set, and resumes the same solver.

## Controlled observations

All measurements used the normalized game catalog, target
`item-proc-battery-5`, a release build and one process per case. Times are elapsed
milliseconds reported inside the process. Disposable JSON reports were retained
under ignored `target/` only during analysis.

### Feasibility and connectivity cost

| Formulation | 16 facilities | 64 facilities | 128 facilities |
| --- | ---: | ---: | ---: |
| Balance only | 11 ms | 11 ms | 12 ms |
| Recipe-arc connectivity | 2,876 ms | >10,000 ms | >10,000 ms |
| SCC connectivity | 314 ms | 1,042 ms | 1,216 ms |

The initial balance-only timing used 1,313 integer variables. Adding explicit
boundary direction and target quantity brought the refined compact kernel to
1,513 variables. SCC connectivity remained compact at about 1,644 variables and
2,548 linear constraints. Direct recipe-arc connectivity used 3,431 variables
and 5,960 constraints before the boundary-direction refinement and was already
too slow for routine generation.

### Objective behavior

Maximizing target throughput with arbitrary external cuts was solved to
optimality in 259 ms, 2,875 ms and 2,483 ms for 16, 64 and 128 facilities. Every
solution assigned all facilities to the final battery recipe and imported its
two ingredients. This is mathematically optimal but unsuitable dataset
variation. Target throughput therefore belongs in the feasibility witness and
recorded observations, not as the default scalar objective.

Restricting external inputs to the ten raw catalog items made 16 facilities
provably infeasible in 78 ms. The 64- and 128-facility cases found no solution in
15 seconds. A full-factory-only restriction is therefore not an acceptable
replacement for path-specific external cuts.

### Lazy cuts and partial factories

With free target quantity, zero waste and at most two imported item kinds, the
16-facility lazy model reached a target-connected active graph after three cuts
in 67 ms. The selected solution used only the battery recipe because its two
ingredients exactly consumed the cut allowance.

Allowing four imported item kinds produced a real partial chain:

```text
plant-moss seed
-> planter
-> moss grinder
-> originium enrichment
-> battery assembly
```

The solution used 16 facilities across four active recipes and zero waste. It
needed three lazy cuts and 9,023 ms. The analogous 64-facility case reached five
cuts but timed out at 15 seconds. Static active-graph reachability is useful as a
cheap separation oracle, but it does not yet prove that positive material flow
on those arcs reaches the target.

## Findings

The global constraint direction is promising, but a monolithic model is not.
The fast part is integer facility capacity plus aggregate material balance. The
expensive part is encoding every possible recipe-to-recipe relevance arc before
the active topology is known.

SCC condensation removes most of that cost, but the game catalog contains one
218-recipe SCC dominated by reversible filling and dismantling operations. SCC
connectivity alone can admit an irrelevant subcycle inside that component. It is
a useful relaxation, not the final correctness authority.

The current source-plan pipeline cannot represent the aggregate model without
loss. It creates path-specific recipe occurrences and rounds facility counts per
occurrence, while the CP model shares production across consumers and rounds the
aggregate workload. Converting a CP result back to that tree can increase the
facility count. A production cutover therefore needs a new aggregate semantic
graph and direct facility-instance expansion.

## Next prototype

Keep the compact CP kernel and add an explicit positive material-flow
decomposition for the selected active recipes. This stage should:

1. allocate each produced item to active consumers, target output or surplus;
2. allocate external cuts to concrete consumer demands;
3. reject or cut active recipes whose positive flow cannot reach the target;
4. expand aggregate workloads directly into facility instances and verify the
   exact requested count; and
5. sample topology with seeded weights or bounded constraints over cut count,
   recipe types and depth instead of maximizing throughput.

Use lazy separation so the common case keeps the 1,500-variable compact model.
Only selected active recipes should receive detailed flow variables. Persistent
workers should reuse the catalog, SCC condensation and base model across sample
requests. The next measurement must include decomposition, instance expansion
and validation; the 11 ms balance solve alone is not an integrated throughput
claim.

## Reproduction

Build the release probe:

```sh
cargo build --release -p aic-cli --bin aic-recipe-cp-probe
```

One representative partial-factory case:

```sh
target/release/aic-recipe-cp-probe \
  --recipes data/game/normalized/recipes.json \
  --target-item item-proc-battery-5 \
  --facilities 16 \
  --connectivity lazy \
  --free-target \
  --max-external-items 4 \
  --max-waste-units 0 \
  --max-lazy-cuts 64 \
  --time-limit-ms 15000 \
  --report-output target/recipe-cp-probe.json
```
