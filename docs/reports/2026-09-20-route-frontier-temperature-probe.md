# Route frontier temperature probe

- Date: 2026-09-20
- Retained checkpoint: replay seed 303
- Checkpoint SHA-256: `3c13a9ae2e75b9300f9e15e5dd633ba3f1a99a4fae499e2f940ce002e3ea8916`
- Probe config SHA-256: `d0056879d8b2dd2e7a2e48886e6b8f46860eb65f4681604a8a8c45e784d0ef37`
- Result SHA-256: `18bbb7eb4718b1f941dd4d4b8f38f621f65f8fac7717368a8d83a017e049d3fa`

## Verdict

Frontier-focused exploration crosses the previously unobserved boundary. A
frozen policy produced 49 valid crossed depth-one continuations from its own
retained depth-zero routes. All four tested temperatures produced successes;
temperature 1.5 had the highest observed count, with 18 successes and 17 unique
successful command paths in 256 attempts that had a legal next action.

This is evidence that the current policy representation and action domain can
express a depth-one solution. It rules out the claim that a representation
change is required before any such success can occur. It also explains why the
ordinary self-roll-in run saw none: the target state was reached rarely, half of
the discovered successful first routes blocked the next option immediately, and
the actionable continuation probability was only about 1.6% to 7.0% in this
probe.

The result does not establish temperature 1.5 as an optimum. The confidence
intervals overlap, the matrix uses one retained checkpoint, and repeated trials
from one prefix are correlated through that fixed prefix. It establishes the
narrower fact needed for the next decision: concentrating attempts at a verified
frontier produces diverse depth-one successes, and moderate extra exploration
is compatible with retaining them.

## Controlled probe

The accepted contract is
[`2026-09-20.01-route-frontier-temperature-probe.md`](../designs/2026-09-20.01-route-frontier-temperature-probe.md).
The replay seed-303 checkpoint, native session, task geometry, route bound,
model weights and catalogs stayed fixed. Only factor sampling temperature varied.
No optimizer or SIL update ran.

The frozen policy sampled successful first routes in two-connection crossed
tasks. Semantic command-path deduplication retained eight distinct prefixes
after 640 discovery attempts. Replaying each prefix through the normal native
session reproduced exactly one committed route. Each prefix then received 64
independent attempts at every temperature.

Temperature used the exact behavior distribution
`softmax(base_log_probability / temperature)` at every conditional factor.
The probe records likelihood and entropy from that distribution rather than
labeling temperature samples with the base-policy likelihood.

## Results

Four prefixes exposed a legal second-route action and four were blocked before
the second route could emit a command. Because blocked availability is fixed by
the prefix, the table shows both the operational rate over all eight prefixes
and the conditional rate over the four actionable prefixes.

| Temperature | All attempts | Actionable attempts | Successes | Unique successes | All-prefix rate | Actionable rate | Actionable Wilson 95% | Mean factor entropy |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 0.5 | 512 | 256 | 4 | 3 | 0.78% | 1.56% | 0.61%–3.95% | 0.3314 |
| 1.0 | 512 | 256 | 12 | 11 | 2.34% | 4.69% | 2.70%–8.01% | 0.4095 |
| 1.5 | 512 | 256 | 18 | 17 | 3.52% | 7.03% | 4.49%–10.84% | 0.4356 |
| 2.0 | 512 | 256 | 15 | 12 | 2.93% | 5.86% | 3.58%–9.44% | 0.4527 |

Success was not confined to one prefix. At temperature 1.5, the four actionable
prefixes produced 2, 6, 5 and 5 successes. The other prefixes were blocked at
every temperature, so increasing entropy cannot repair a first route that has
already consumed every legal start for the second route.

Among actionable attempts, failures split between active dead ends and the
24-command route bound. This means the next training slice has both kinds of
credit available; it does not need a global feasibility check or generated
route to distinguish them.

## Interpretation

The two controls discussed before the probe are both supported, but they act at
different boundaries:

1. Frontier admission should retain diverse self-generated prefixes that leave
   the observed next option locally actionable. The four blocked prefixes are
   valid first-route successes, but they provide no exploration opportunity for
   depth one.
2. Exploration should rise when a new actionable frontier opens. The empirical
   success count increased from temperature 0.5 through 1.5 and then decreased
   at 2.0. This is the expected discovery-versus-consolidation tradeoff, although
   this one-checkpoint matrix does not determine a universal temperature.

This matches the useful parts of Go-Explore's return-to-frontier mechanism,
GoalGAN's focus on an intermediate-success region, and target-entropy control in
SAC. PPO remains compatible when one temperature is fixed for a collection and
its matching behavior likelihood is retained through the PPO epochs.

## Decision and next bounded experiment

Keep the current representation and add a research-only frontier curriculum
before considering a route-local encoder. Compare the existing two-route
curriculum against one treatment over three initialization seeds. The treatment
may oversample only self-generated, semantically unique, locally actionable
depth-zero prefixes; it must still evaluate from fresh roots. Use temperature
1.5 for discovery until unique depth-one successes enter replay, then return to
temperature 1.0 for consolidation. Freeze the chosen temperature for each
rollout and all PPO epochs consuming it, and retain its exact behavior
likelihood.

The treatment passes only if it produces fresh-root greedy crossed depth-one
success in more than one seed. A replay-only gain from archived prefixes is not
enough. Do not integrate route options into full-factory training before that
gate passes.

## Verification and retained evidence

The exact source was built on `ashrimp-arch` under
`~/Srv/aic-route-frontier-probe-20260920`. `cargo check -p aic-cli --features
learning` passed. The focused temperature-distribution and Wilson-bound tests
passed with `aic-learning/libtorch`. The optimized CUDA probe completed in 2.4
seconds after build.

The release binary SHA-256 is
`4cf37b14ea3b6c653edcc12a6aa87d981c63722b18a668c84c736220861b44c7`.
The retained server output is
`~/Srv/aic-route-frontier-probe-20260920/artifacts/frontier-seed303`; a local
ignored copy is under `artifacts/route-frontier-probe-20260920`. It contains the
probe config, exact prefix command manifest, JSON report, self-contained HTML
report and file-identity manifest.
