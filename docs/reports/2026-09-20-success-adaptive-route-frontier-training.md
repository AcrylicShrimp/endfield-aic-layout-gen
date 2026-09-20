# Success-adaptive route frontier training

- Date: 2026-09-20
- Treatment template SHA-256: `918d4aacd27c51ad84072df843b0316b897b902e5a82bf1bf176b9607530cb19`
- Control: retained route SIL v3 runs
- Treatment: route SIL v3 plus a self-generated depth-one frontier
- Seeds: 101, 202 and 303

## Verdict

The frontier curriculum works as a collection and optimization mechanism, but
it does not pass the fresh-root transfer gate. All three seeds admitted native,
policy-generated crossed depth-zero prefixes. Across 2,176 targeted depth-one
attempts, the treatment produced 47 valid and semantically distinct depth-one
successes. Seeds 101 and 202 reached the consolidation phase. No seed produced
a fresh-root greedy crossed depth-one success at any evaluation.

The preregistered pass condition required fresh-root greedy depth-one success in
at least two of three seeds. The observed result is zero of three. The treatment
therefore rejects its training hypothesis despite successfully crossing the
frontier during focused collection.

The failure is specific: local exploration and exact on-policy optimization can
learn successful continuations from restored prefixes, but the learned behavior
does not transfer through the policy's own preceding route. The next bounded
question is state-distribution transfer during consolidation, not whether the
current decoder can express the second route.

## Implemented contract

The accepted design is
[`2026-09-20.02-success-adaptive-route-frontier-training.md`](../designs/2026-09-20.02-success-adaptive-route-frontier-training.md).
The implementation adds the following opt-in path while leaving old configs
unchanged:

- a 32-entry buffer of semantically deduplicated, self-generated crossed
  depth-zero prefixes;
- fresh native reconstruction and local next-option validation before admission;
- causal admission after the update's PPO and SIL epochs;
- a fixed 64-session update budget split between fresh roots and restored
  frontier starts;
- a Wilson-lower-bound controller with bounded 8-to-32-session allocations;
- temperatures 1.5, 1.0 and 0.5 for discovery, consolidation and mastery;
- exact per-choice temperature retention and factor likelihood recomputation in
  mixed-temperature PPO minibatches;
- base-temperature likelihoods for historical SIL labels;
- joint shape-by-roll-in-depth metrics and fresh-root greedy mastery gating.

Every retained prefix consists only of accepted commands sampled by the policy.
Reconstruction replays those commands through a new native session and requires
exactly one committed route, no provisional route and a nonempty depth-one
action domain. No route solver, demonstration, repair or global feasibility
oracle contributes a prefix or continuation.

## Controlled comparison

The treatment retains the v3 model, optimizer, replay, task generator, reward,
route bound, 32 updates, 64 sessions per update, four PPO epochs and fixed
64-session held-out panel. Each run therefore collects exactly 2,048 training
sessions and executes 128 optimizer steps. The initialization seed and the new
frontier cursor namespace vary together as 101/920101, 202/920202 and
303/920303. Task, evaluation and SIL replay seeds remain fixed.

The treatment configs have these SHA-256 identities:

| Seed | Config SHA-256 |
| ---: | --- |
| 101 | `918d4aacd27c51ad84072df843b0316b897b902e5a82bf1bf176b9607530cb19` |
| 202 | `e5326246d70c66338e221817a4daa1283170221ed4dcf546d16370a5644b2c34` |
| 303 | `966b456d87049478f7cf8bdd4596109a814a9a1c95507f96d0819f05de5030d4` |

## Frontier operation

| Seed | Prefix candidates | Admitted | Duplicate | Locally blocked candidates | Root sessions | Frontier attempts | Successes | Unique successes | Success rate | Wilson 95% | Final phase |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| 101 | 5 | 4 | 1 | 12 | 1,312 | 736 | 14 | 14 | 1.90% | 1.14%–3.17% | consolidate |
| 202 | 1 | 1 | 0 | 11 | 1,312 | 736 | 29 | 29 | 3.94% | 2.76%–5.60% | consolidate |
| 303 | 2 | 2 | 0 | 7 | 1,344 | 704 | 4 | 4 | 0.57% | 0.22%–1.45% | discover |
| **Total** | **8** | **7** | **1** | **30** | **3,968** | **2,176** | **47** | **47** | **2.16%** | — | — |

Every admitted prefix remained actionable when restored; targeted collection
recorded zero blocked-before-action sessions. Seed 101 first admitted a prefix
at update 9, reached eight unique successes at update 17 and entered
consolidation at update 18. Seed 202 admitted at update 9 and entered
consolidation at update 17. Seed 303 admitted its first prefix at update 10 and
remained below the discovery target through update 32.

The probability budget behaved as intended. With sparse success and a low
Wilson lower bound, it saturated at the configured maximum of 32 attempts per
update. It remained bounded when success was rare and never changed the total
64-session update budget.

## Fresh-root result

### Greedy evaluation

| Arm | Seed | Crossed depth zero | Crossed depth one | All route successes | Full sessions |
| --- | ---: | ---: | ---: | ---: | ---: |
| Retained v3 control | 101 | 8/32 | 0 | 88/112 | 40/64 |
| Retained v3 control | 202 | 0/32 | 0 | 80/112 | 32/64 |
| Retained v3 control | 303 | 8/32 | 0 | 88/112 | 40/64 |
| Frontier treatment | 101 | 8/32 | 0 | 88/112 | 40/64 |
| Frontier treatment | 202 | 0/32 | 0 | 80/112 | 32/64 |
| Frontier treatment | 303 | 0/32 | 0 | 80/112 | 32/64 |

The treatment records `crossed-depth-1` directly. Its maximum fresh-root greedy
depth-one success count was zero at every evaluation in all seeds. The retained
controls predate the joint metric, but their requested-connection breakdown
shows that every crossed greedy success is a single-connection depth-zero
session, as established in the v3 report.

Pooled greedy route success decreases from 256/336 in the control to 248/336 in
the treatment. Pooled crossed success decreases from 16/96 to 8/96 because seed
303 loses its single-route crossed behavior. Aligned greedy behavior remains
240/240 in both arms. The treatment therefore provides no hidden aggregate gain
that could offset the failed depth-one gate.

Stochastic evaluation also records zero crossed depth-one successes: 0/3, 0/2
and 0/3 reached attempts for seeds 101, 202 and 303. This rules out a narrow
greedy-selection artifact in the final checkpoints.

## Interpretation and decision

The frozen-policy probe and this training result answer different questions.
The probe established that actionable prefixes admit diverse second-route
solutions. This experiment confirms that PPO can consume those solutions under
the exact temperature behavior distribution: all 384 optimizer epochs completed
with finite loss, KL, clip fraction and gradient metrics. The missing result is
composition from a fresh root.

Focused frontier practice replaces up to half of each update's root sessions.
After success discovery, the current consolidation phase continues almost
entirely from restored depth-one states. That distribution teaches the second
route conditional on archived first routes, but supplies little paired practice
for producing a compatible first route and then continuing from its current
occupancy. Seed 303 also shows that this reallocation can weaken the preceding
crossed skill.

Keep the frontier and exact-temperature machinery as bounded research
infrastructure, disabled by default. Do not integrate route options into
full-factory training because the fresh-root gate failed. The next experiment
should change only consolidation supply: once discovery succeeds, spend its
budget on fresh two-route crossed sessions seeded from the same task stratum,
while keeping a smaller restored-prefix rehearsal stream. Gate and evaluate it
with the same fresh-root `crossed-depth-1` metric. Do not add a new encoder,
solver-generated prefix or larger unbounded attempt budget yet.

## Verification and retained evidence

Local formatting and the default workspace check pass. On `ashrimp-arch`,
`cargo check -p aic-cli --features learning` passed, followed by 14 focused
constructive tests with `aic-learning/libtorch`. The tests include a mixed batch
with temperatures 0.5 and 2.0 whose recomputed behavior log probabilities match
the stored sampling log probabilities. A bounded CUDA smoke run completed the
same CLI workflow before the three treatment runs.

The release binary SHA-256 is
`2450137ad3d75209f2e35a44d7d49ad6b6dc17f2e94f2b83af78027c1ef4fa88`.
The exact treatment outputs remain on the training server at:

```text
~/Srv/aic-route-frontier-training-20260920/artifacts/route-frontier-v4-seed-101
~/Srv/aic-route-frontier-training-20260920/artifacts/route-frontier-v4-seed-202
~/Srv/aic-route-frontier-training-20260920/artifacts/route-frontier-v4-seed-303
```

Together they occupy about 18.6 MiB and contain each resolved config, held-out
task panel, parameter order, metrics stream, final model and report. The server
had 12,414,414,848 bytes available after retention.
