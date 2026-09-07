# One-step action freedom: measured capability and remaining obstruction

Status: development capability experiment complete; overall goal active and not ready.

## Scope and execution

The [contract](capability-contract.md) defines finite raw action domains, independent
one-step trials, preservation of the original production request, explicit budget accounting,
and the distinction between local repair failures and global impossibility. The existing
RL environment and production planner are unchanged. This probe does not train a policy.

The four-root pilot covers two small development factories (Citron, Iron Nine), the loose
59-facility Heavy Xiranite constructor and its strongest recorded geometric incumbent.
It ran 48 root/level cases and 24,475 raw proposals in about 54.1 seconds of summed level
time, with 3,633 candidate validation calls. Every generated candidate that reached validation
passed. Root checks precede timed levels; all candidate work and screening are charged.

Several pilot rows hit the aggregate route or proposal limit. In particular, pair-T4 and
pair-T8 initially visited identical prefixes and therefore did **not** compare longer moves.
This was corrected with a larger-budget, complete-domain Heavy Xiranite follow-up: 14 levels,
37,705 proposals, 1,789 candidate validations, approximately 20.1 seconds. Every declared action
was visited and no per-action routing expansion limit was hit. Counts and scores reproduced
in a second run; timings are observations, not a statistically controlled speed ranking.

Machine-readable evidence, source hashes and retained-witness/trace hashes are in
[capability-results.json](capability-results.json). Generated artifacts live under
`target/layout-capability-02/` and `target/layout-capability-strong-02/`.

## What the new freedom buys

| Factory / domain | Best area | Best transport tiles | Result |
| --- | ---: | ---: | --- |
| Citron, original | 230 | 24 | Reference |
| Citron, translation up to 8 cells | 184 | 24 | Additional distance beyond 2 gives no better sampled one-step score |
| Citron, rotation plus shift up to 2 | 176 | 24 | Better area than translation-only domain |
| Iron Nine, original / translations | 364 | 35 | No one-step translation improvement |
| Iron Nine, rotation | 350 | 35 | Rotation exposes area improvement |
| Iron Nine, port reassignment | 364 | 33 | Secondary-objective improvement |
| Heavy Xiranite, original strong | 2,132 | 392 | Reference |
| Heavy Xiranite, rotation plus shift | 2,132 | 388 | Secondary-objective improvement only |
| Heavy Xiranite, port reassignment | 2,132 | 388 | Secondary-objective improvement only |

These are best observed candidates, not learned-policy expected performance. All candidates
start from the same root in each factory. The strong rotation-plus-shift result has common
score `(2132,388,145)` versus `(2132,392,147)` initially. Its empty fraction is **29.8780%**:
removing four transport cells without reducing the box increases emptiness, as the density
metric correctly reports. It does not pass the 1% goal and must not be advertised as denser.

On the loose Heavy Xiranite root, groups up to 8/16 members found an area reduction from
12,900 to 12,771 while the sampled singleton/pair actions improved only transport cost.
Those larger-root domains were resource-limited, so their relative ranking remains partial.

## Why merely adding action tokens is insufficient

Complete strong-root domains:

| Level | Raw actions | Static rejection | Repair failure | Unique valid states | Strictly improved states |
| --- | ---: | ---: | ---: | ---: | ---: |
| Single, distance 1–2 | 472 | 372 | 36 | 64 | 0 |
| Pair, distance 1–8 | 4,160 | 3,890 | 102 | 168 | 0 |
| Groups up to 16, distance 1–8 | 11,456 | 11,117 | 152 | 187 | 0 |
| Rotation only | 177 | 94 | 62 | 21 | 1 |
| Rotation + distance 1–2 | 1,593 | 1,196 | 275 | 122 | 3 |
| Rotation + distance 1–8 | 5,841 | 5,245 | 415 | 181 | 3 |
| One terminal port change | 533 | 146 | 60 | 327 | 37 |

The static failures are not all body overlap. Even in the singleton domain, 51 candidates
hit retained terminal anchors, 51 hit fixed anchors with their facility body, 40 put an
endpoint inside a body, and 5 required shared-terminal owners to move together. The remaining
static rejections are 181 body overlaps and 44 current-extent violations. The old policy's
body/bounds-only mask cannot distinguish these additional causes.

These raw exhaustive-trial rejection fractions are **not learned-policy waste rates**.
They show why a uniformly enlarged flat action space is an insufficient design. Port actions
have substantially more valid opportunities here, while longer translation/rotation shifts
add predominantly collisions. Bounded two-step or coordinated actions may still use valid
non-improving states; zero one-step improvement does not make a freedom intrinsically useless.

Changing group junction ownership from internal to downstream changes some failure identities,
but did not improve the strong root's score. Standalone rerouting produced 101 unchanged
results, three unique non-improving states and one duplicate across 105 actions. Together
with the preceding fixed-chain length audit, this points toward changing packing/endpoints
rather than just asking the same fixed-endpoint router to search harder.

## Next experiment, before more training

1. Add an explicit move/rotation/port-then-strip-compaction interaction, with both intermediate
   and final validation and all work charged. Existing straight-strip contraction can reduce
   area after an action exposes a removable strip; it is deliberately absent from this pure
   one-step matrix. Include compaction-only as the control.
2. Probe placements aligned to measured voids or retained facility contacts, including
   simultaneous X/Y shifts. Cardinal shifts of at most 8 cells cannot reach every visible
   void. Report generated coverage rather than silently treating a geometric shortlist as
   exhaustive. Then test paired port/anchor changes if those concrete candidates need them.
3. If larger geometry changes become reachable, compare charged full-static masks and richer
   occupancy/port observations against the old body/bounds mask, followed by factorized
   action selection and imitation-plus-RL versus RL from scratch. Candidate outcomes must
   remain training labels, never inference features.

External terminals are fixed by this local kernel, and multiple facility/terminal groups
contribute to each occupied boundary. This is an implementation/neighborhood restriction,
not a production W/H ceiling or a global proof. Multi-step state changes, strip compaction,
different group shapes, boundary relocation and alternative routing choices remain outside
this measurement. The user goal is still open; no goal-level impossibility is established.

## Verification

Nine capability tests cover the revised semantic identity, raw-domain inclusion/order,
catalog rotations, static screening, independent validation, frozen-root preservation,
proposal/expansion limits, late-result rejection and fatal validator errors. Snapshot tests
verify coverage/counter accounting, budgets, incumbent retention and result scope. CLI runs
above passed, and duplicate output paths were manually checked to fail without overwriting.

```sh
node --test docs/benchmarks/layout-production/*.test.mjs
node docs/benchmarks/layout-production/capability.mjs --plan docs/benchmarks/layout-production/capability.json --output-directory target/layout-capability-02
node docs/benchmarks/layout-production/capability.mjs --plan docs/benchmarks/layout-production/capability-strong.json --output-directory target/layout-capability-strong-02
cargo check --workspace
cargo test --workspace
```

Use fresh output paths when reproducing. No dependencies or Rust code changed. Before
committing, compared this slice to the accepted constructive planner design and repository
policy: all states used as factory results are complete and independently validated, the
original request is preserved, score priority is unchanged, and no global exact solver,
facility-only production fallback or caller-supplied canvas bound was introduced.
