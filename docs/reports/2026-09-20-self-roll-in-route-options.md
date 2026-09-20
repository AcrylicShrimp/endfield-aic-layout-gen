# Self-roll-in route-option experiment

- Date: 2026-09-20
- Source commit: `5e9fc962fb57d38a90f3bb8fb7d7c35afc365cd4`
- Release binary SHA-256: `3a68e8861c707595d40b2fdee9a9a9bf1b8ae4dc577f9c41de2b30d6802fa520`

## Verdict

Training one physical connection as one PPO episode shortened routing credit
assignment and produced real self-roll-in. A successful option committed its
path through the native session, and the next option observed those exact cells
as occupied tokens. All three final policies reached nonempty roll-in states,
and two reached depth three, without generated paths, demonstrations or path
repair.

The learned option is not ready for the full constructive policy. After 32
updates, the macro mean greedy route success across three initialization seeds
was `0.611111`, but it ranged from `0.333333` to `0.785714`. More importantly,
greedy crossed-route success was only 8 of 96 attempts (`0.083333`) and all
eight successes were single-connection cases from seed 101. No seed completed
a crossed route after an earlier route had been committed. A 128-update seed-101
extension also finished with zero greedy crossed success, so training duration
does not explain the missing behavior.

The experiment therefore passes the self-roll-in architecture check and fails
the route-competence gate. The next bounded question is whether a route-local
observation and decoder, centered on the selected endpoints while retaining
exact committed occupancy, can master aligned and offset single routes before
unrelated graph context is restored. Reward tuning, a larger graph curriculum
or full-factory integration should wait for that result.

## Implemented experiment boundary

The separate `learning train-constructive-routes` command reuses the existing
constructive attention model, conditional action decoder, PPO learner, native
session and sparse occupancy representation. It adds a route-only task stream
and collection boundary:

1. External configuration selects a facility, transport kind, canvas, route
   count and geometry. The task generator places one to four facility pairs but
   does not create ports or transport cells.
2. Each PPO episode exposes only the selected connection's `begin-route`, port,
   `extend` and `connect` choices. The policy owns every geometric decision.
3. Success commits through the normal native session. The same session starts
   the next connection, whose observation includes every previously committed
   route cell. Failure rolls back the provisional route and stops that session.
4. A 24-command option timeout replaces an explicit cold-policy abort action.
   If the preceding route leaves the next option with no legal starting port,
   the collector records that blocked start and applies its configured
   continuation penalty to the preceding terminal transition.

One connection is one return horizon even when several options reuse a native
session. The domain cannot place facilities or boundary terminals, select a
different logical connection, move a facility or alter a committed route.
Extension lengths `1`, `4`, `5`, `10` and `15` include the experiment's common
coordinate deltas, so an offset route does not require an arbitrary binary
decomposition.

The reward gives a constant success bonus and uses discounted potential shaping
inside the option:

```text
Phi(s) = -Manhattan(open_endpoint, target) / canvas_span
shape(s, next) = gamma * Phi(next) - Phi(s)
```

The terminal potential is forced to zero. This closes the shaping ledger on
success and failure, preventing an extend-then-timeout trajectory from keeping
approach reward. One terminal remaining-distance penalty distinguishes failed
routes without paying per occupied cell. A unit test verifies discounted ledger
closure; other focused tests verify the restricted action domain, absence of
explicit abort, committed occupancy in the next option, and reachability of an
authored offset route.

## Controlled protocol

The retained configuration is
[`config.json`](../benchmarks/direct-constructive-route-options/config.json).
Only `initialization_seed` changed among the three arms.

| Field | Value |
| --- | ---: |
| Model width / message rounds | 128 / 4 |
| Updates / sessions per update | 32 / 64 |
| PPO epochs / minibatch size | 4 / 128 |
| Route command bound | 24 |
| Fixed evaluation sessions / interval | 64 / every 4 updates |
| Connections opened by curriculum | 1 to 4 |
| Learning rate / entropy coefficient | 0.003 / 0.01 |
| Initialization seeds | 101, 202, 303 |
| Evaluation seed / task seed | 710101 / 110101 |

Evaluation used the same fixed 64-session panel for every arm. The panel mixes
one to four connections and aligned or reversed target rows; its SHA-256 is
`17578710b436c890ed1debbcbd1c86ef0d2680938009b05b0358500cea85090f`.
Greedy evaluation measures the deployable decoded policy, while stochastic
evaluation shows whether useful choices remain in the learned distribution.

The initialization seeds intentionally expose a limitation of straight-route
tasks: seed 303's untrained greedy decoder already followed a lucky constrained
action sequence and solved all aligned routes, while seeds 101 and 202 solved
none. Crossed routes were zero for all three untrained greedy policies.

## Three-seed result

| Seed | Initial greedy | Final greedy | Final stochastic | Greedy aligned | Greedy crossed | Greedy full sessions | Stochastic blocked starts |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 101 | 0/64 options | 88/112 (0.785714) | 36/86 (0.418605) | 80/80 | 8/32 | 40/64 | 12 |
| 202 | 0/64 options | 32/96 (0.333333) | 10/73 (0.136986) | 32/64 | 0/32 | 0/64 | 23 |
| 303 | 80/112 (0.714286) | 80/112 (0.714286) | 63/103 (0.611650) | 80/80 | 0/32 | 32/64 | 6 |
| Macro mean | 0.238095 | 0.611111 | 0.389080 | — | — | 24/64 | 13.67 |

The pooled final greedy result was 200 successes in 320 attempted options
(`0.625000`). Aligned routes succeeded in 192 of 224 attempts (`0.857143`),
whereas crossed routes succeeded in 8 of 96. The aggregate score therefore
mostly measures straight routing and successful continuation through additional
straight routes.

The roll-in-depth breakdown confirms that later policies saw their own earlier
work rather than only empty canvases:

| Seed | Greedy depth 0 | Greedy depth 1 | Greedy depth 2 | Greedy depth 3 | Maximum reached |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 101 | 40/64 | 24/24 | 16/16 | 8/8 | 3 |
| 202 | 24/64 | 8/24 | 0/8 | — | 2 |
| 303 | 32/64 | 24/24 | 16/16 | 8/8 | 3 |

Depth is conditional: an option at depth `k` exists only when the same policy
successfully committed the preceding `k` routes. Thus the perfect later-depth
rates for seeds 101 and 303 do not establish general congested routing. They
show that the policy continued through the subset of sessions whose earlier
aligned routes succeeded. Reversed-row sessions stopped at their first crossed
decision.

Stochastic evaluation was also unstable. It reached depth three for seeds 101
and 303, but crossed successes were only 5/35, 1/33 and 0/32. The 41 recorded
blocked starts show why the continuation penalty is necessary: a locally valid
committed route can consume all ports needed by the next option. The final
greedy policies avoided blocked starts, but did so mainly by selecting the
simple aligned behavior rather than learning multi-route crossed geometry.

## Ablations and failure localization

Small controlled runs changed the design before the final comparison:

- The first smoke run completed one route and exposed its occupancy at depth
  one, proving the collection and commit path end to end.
- An early run silently lost sessions whose next connection had no legal start.
  Recording `blocked_before_action` and charging the preceding successful option
  made that failure observable and assigned it to the route that caused it.
- Removing offset tasks learned aligned routes and later depths but provided no
  evidence about turns or detours.
- Potential shaping with offset tasks still produced zero crossed completion.
- Adding a terminal remaining-distance penalty supplied a one-shot near-miss
  signal but did not by itself solve crossed routing.
- Removing explicit abort prevented the cold policy from selecting an immediate
  exit and allowed seed 101 to learn the single offset case. Seeds 202 and 303
  did not reproduce it.
- Extending seed 101 to 128 updates with the same no-abort domain ended at
  greedy crossed success `0/32`; its final aligned result was `72/80`. The
  longer policy even lost the earlier single-offset behavior.

The contrast within seed 101 localizes the likely representation problem. The
same learned decoder can solve an offset route when it is the only connection,
but fails when unrelated facilities and connections enlarge the pooled graph
context. More PPO updates amplify or erase that brittle choice rather than
turning it into a stable routing rule.

## Decision and next experiment

Do not integrate this option into full-factory training. It would add a nested
policy boundary while preserving a known zero-success region for the situations
that motivated self-roll-in.

The next experiment should keep the successful boundaries from this work:
native legality, exact committed occupancy, one-connection PPO episodes,
terminal shaping closure, blocked-start responsibility and the fixed held-out
panel. It should change only the policy view and curriculum:

1. Express facility ports, open endpoint, target endpoint and occupied cells in
   a coordinate frame local to the selected connection.
2. Decode only route actions from that selected context instead of pooling every
   unrelated graph entity into the option decision.
3. Require multi-seed mastery of aligned and offset single routes before opening
   the second connection, then require mastery at each new occupancy depth.
4. Reintroduce broader graph context only after measuring whether it changes a
   previously mastered fixed panel.

This tests a specific causal hypothesis. Repeating reward adjustments or simply
running longer does not, because both already failed to make crossed behavior
stable.

## Evidence and reproduction

Run the seed-101 arm with the retained configuration; copy it and change only
`initialization_seed` to `202` and `303` for the other arms:

```sh
aic learning train-constructive-routes \
  --config docs/benchmarks/direct-constructive-route-options/config.json \
  --catalog-root data/game/normalized \
  --output target/direct-constructive-route-options-seed-101 \
  --device cuda:0
```

The catalog SHA-256 was
`d64fc5916e2d57c3052dd8f7eeefd85b4211ad222553c450391b601d64a51cca`.
All reports declare strict determinism, status `completed`, 32 updates and 128
optimizer steps.

| Seed | Config SHA-256 | Metrics SHA-256 | Report SHA-256 | Learner SHA-256 |
| ---: | --- | --- | --- | --- |
| 101 | `e3720d24d112c3b994509c0577fb638e59e5ce01f3b67f21d48db0eba8bcf990` | `c1214bd5948cb4a9d0127d6cabe225729b539e0005de237edcd14fa60bc94a16` | `e409b946179591891896998642ab09d39377ff11ae9fb51d657b18a134b5b766` | `9a1a9f25d2f8e9e16cbf12ee60ecb79dd770303a58e7a39e94c6e3815796d2da` |
| 202 | `9aa924fbd16ac70d1b00f5da5b0ca17126530429d714e881ca10c6b2c9b6637b` | `64fc34470188802502e667f9da318a046d55a586c1d22d75265f52c42980fcf5` | `20131fe7d5b8b9c92ce024ace5db89e7068ce54a0e4648a8cebe30c94d20a488` | `48585d8341981a7f1887aef4ae07c5927be6ce30dc810562075a3234b351b634` |
| 303 | `32e07efed1fbc439ec8a100ab1a6c4e348fed106d76d27d8f6cfc01b01b8b6a0` | `5cefbbad75f1c4aaccbfddacd7559b7525d1f6e5fa3047dc8b652362f90362e4` | `afb76882c4f2726820d82a416b65377a43bf70dace494c07c9354c87dcc9e993` | `1a24d89fb0332e39153e9a5376ff19361aeaa2762e124746e9e431d8d924ee52` |

Training consumed 2,255 / 2,220 / 2,334 route options and 30,769 / 30,629 /
30,976 policy turns for seeds 101 / 202 / 303. The retained 128-update ablation
used config SHA-256
`76b5700d2e8c25d86cb98310ceef73224073b4c0d9898baef946d5c2e724280a`
and learner SHA-256
`158560dad1269a572c9bfc88d2cb145395c9dc3b7297ce136d4df46f46257409`.

Local focused native tests passed. On `ashrimp-arch`, `cargo check --workspace`
passed and `RUST_MIN_STACK=16777216 cargo test --workspace` passed. The larger
stack was needed for an existing CLI test that overflowed its default test
thread stack; the route-option tests themselves passed in that workspace run.
