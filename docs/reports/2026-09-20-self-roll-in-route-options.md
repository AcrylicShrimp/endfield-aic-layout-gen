# Corrected self-roll-in route options and successful-experience replay

- Date: 2026-09-20
- Evaluation panel SHA-256: `17578710b436c890ed1debbcbd1c86ef0d2680938009b05b0358500cea85090f`
- Catalog SHA-256: `d64fc5916e2d57c3052dd8f7eeefd85b4211ad222553c450391b601d64a51cca`

## Verdict

The route option remains a useful curriculum unit, but neither the corrected PPO
control nor route-specific successful-experience replay can yet complete the
multi-route crossed cases needed by a full factory. The retained conclusion is
therefore **do not integrate this option as the full constructor's routing
stage**.

Two collector defects invalidate the earlier version of this report. An empty
action domain after an active route had already emitted commands was recorded as
a blocked next start, so its failed trajectory was discarded. The curriculum
also coupled crossed geometry to later connection counts and did not reliably
present a single offset route before multi-route tasks. The corrected collector
keeps active dead ends as terminal failures, reserves `blocked_before_action`
for a truly empty option, and presents balanced aligned/crossed shapes across
all available connection counts after a four-update aligned warmup.

With those corrections, the no-replay control completed 8 of 96 final greedy
crossed attempts across three seeds. A bounded, quality-retaining replay buffer
completed 16 of 96. The apparent gain is limited: all 24 successes from both
arms were the first and only route in single-connection crossed sessions. No arm
completed a crossed option after its own earlier route had been committed in
held-out greedy evaluation, and the replay arm never admitted such a training
success. Replay stabilized one offset behavior in two seeds and improved aligned
stochastic continuation, but it did not cross the multi-route cliff.

This supersedes the old claim that blocked-start counts established widespread
port starvation and the old recommendation to replace the representation next.
The corrected evidence localizes the unresolved boundary more narrowly: useful
single-offset paths are sometimes discovered and retained, while no successful
crossed continuation at roll-in depth one is observed. The result does not yet
distinguish policy representation, curriculum supply, or optimization as the
cause of that remaining failure.

## Collector correction

A route observation can fail with an empty domain in two different states:

1. If the current option has no transitions, the option was blocked before its
   first action. The collector records it separately and charges the immediately
   preceding successful route, because that committed occupancy is the observed
   cause for this actual ordering.
2. If the current option already has transitions, it is an active-route dead end.
   The collector closes the potential-shaping ledger, applies the ordinary
   timeout and remaining-distance penalties, marks the final transition
   terminal, and retains the trajectory for PPO.

This correction adds no global next-route feasibility test and does not search
alternative route orderings. It uses only the normal action domain encountered
by collection. The corrected three-seed control recorded 1,983 active dead ends
and 140 truly blocked starts during training; these are now distinct events.

The corrected curriculum first presents aligned one-route work, then exposes a
single crossed offset on the full four-row geometry. As connection depth opens,
it balances the Cartesian product of available connection counts and aligned or
crossed shape. A crossed task remains a full session whose later options observe
exact occupancy committed by the same policy.

## Route-specific successful replay

The replay implementation is deliberately local to route options. It does not
reuse the old factory-edit replay archive or introduce a new solver, path
generator, or feasibility oracle.

- Admission unit: one successful route-option trajectory. Every retained label
  stores the exact historical observation, conditional action factors, and
  discounted option return.
- Causal order: an update samples only entries admitted by earlier updates. The
  current collection enters the buffer after all four PPO/SIL epochs finish.
- SIL objective: `max(return - current_value, 0)` is detached from the critic and
  weights the historical joint command log probability. The replay term updates
  only actor parameters. Each sampled sequence gets equal total weight regardless
  of its command count.
- Semantic identity: the serialized observation input, legal domain, and selected
  factor path are hashed. Duplicate paths do not multiply buffer occupancy; a
  duplicate replaces the retained copy only when its observed downstream quality
  is better.
- Diversity: draws and capacity are stratified by aligned/crossed shape and exact
  self-roll-in depth. The 64-sequence, 1,024-label bounds are sufficient for the
  eight possible shape/depth strata but force actual retention decisions.
- Retention quality: an option that immediately blocks the next start ranks
  lowest. More observed downstream continuations rank above fewer, eventual full
  session completion breaks continuation ties, and option return and accepted
  command count break remaining physical ties. Newly admitted entries receive a
  short grace period. Measured SIL utility is used only after four draws and only
  after downstream quality; stale mature low-utility entries lose later ties.

The policy never evaluates unseen follow-up orderings for quality. `observed
continuations` and `full session` come from the ordinary collector outcome of the
same native session.

## Controlled comparison

Both arms use 32 updates, 64 sessions per update, four epochs, minibatches of
128, model width 128, four message rounds, route bound 24, the same task and
evaluation seeds, and initialization seeds 101, 202, and 303. They use the same
fixed 64-session held-out panel. The only treatment addition is replay with SIL
coefficient 0.05 and the bounds described above.

The corrected control template is
[`direct-constructive-route-options-v2/config.json`](../benchmarks/direct-constructive-route-options-v2/config.json),
SHA-256 `d51e10e5019cd306e29e51604daa4d1583fc598cfb0a3d93a8c5eb66ce228bc1`.
The replay template is
[`direct-constructive-route-options-v3/config.json`](../benchmarks/direct-constructive-route-options-v3/config.json),
SHA-256 `058c494118a0a664336b6d040ea708390a921b4396c54b8684c4576841dae7a4`.
Only `initialization_seed` changes between seed copies.

## Held-out result

### Greedy evaluation

| Arm | Seed | Route successes | Crossed successes | Full sessions |
| --- | ---: | ---: | ---: | ---: |
| Corrected PPO | 101 | 80/112 | 0/32 | 32/64 |
| Corrected PPO | 202 | 80/112 | 0/32 | 32/64 |
| Corrected PPO | 303 | 88/112 | 8/32 | 40/64 |
| Replay | 101 | 88/112 | 8/32 | 40/64 |
| Replay | 202 | 80/112 | 0/32 | 32/64 |
| Replay | 303 | 88/112 | 8/32 | 40/64 |

The pooled route success changes from 248/336 (`0.738095`) to 256/336
(`0.761905`). Pooled crossed success changes from 8/96 to 16/96. In every seed,
aligned success is 80/80. The replay difference is therefore exactly one
single-connection crossed session in seed 101. Seed 303 already has the same
single-route behavior in the corrected control. Seed 202 remains at zero.

The requested-connection breakdown establishes the limit. A successful
single-connection crossed session contributes the extra eight successes at
requested count one. Counts two through four contain only the aligned sessions'
successes. No crossed session reaches a successful option at depth one.

### Stochastic evaluation

| Arm | Pooled routes | Pooled crossed | Full sessions |
| --- | ---: | ---: | ---: |
| Corrected PPO | 41/216 | 4/98 | 17/192 |
| Replay | 118/271 | 4/100 | 39/192 |

Replay substantially preserves aligned stochastic behavior and therefore opens
more later options. It does not improve pooled stochastic crossed successes.
This separation matters: the actor learned to revisit known easy trajectories,
while the missing crossed continuation stayed missing.

## Replay operation

The final replay buffers contain exactly 64 sequences in every seed. They retain
50/14, 44/20, and 46/18 aligned/crossed sequences for seeds 101/202/303. Their
largest observed serialized sizes are 2.42, 3.10, and 2.90 MB. Capacity causes
77, 42, and 61 actual evictions, so the retention policy is exercised rather
than merely configured.

Across 124 optimizer epochs, the three seeds draw 948/872/892 sequences and
7,497/8,291/7,743 labels. Positive SIL labels are 7,456/8,289/7,736. Update one
has zero replay labels in every run, and its collected successes become eligible
only at update two. Final buffers contain aligned depths zero through three as
available, but only `crossed-depth-0`; no `crossed-depth-1` entry was ever
admitted. Because capacity eviction protects underrepresented strata, this
absence is evidence that collection never produced such a success, not that the
buffer discarded the entire stratum.

## Decision and next bounded question

Keep route-specific replay as experimental infrastructure because it is bounded,
causal, and demonstrably preserves some successful behavior. Do not treat the
0.05 coefficient or 64-sequence capacity as adopted training defaults, and do
not integrate the route option into full-factory training yet.

The next experiment should target the first crossed continuation directly while
preserving the full-factory objective: require a policy to retain single-offset
success, then open a two-route crossed session and measure whether depth-one
success ever appears. Use the same native self-roll-in and actual-order blocked
signal. A bounded curriculum gate may change task supply, but it must not preload
a routed prefix, call a path solver, or run a global next-route feasibility
check. If no depth-one success is discovered despite adequate depth-zero mastery,
then compare a route-local encoder against the existing representation on that
same two-route boundary.

## Verification and retained evidence

The exact treatment source was built and run under
`~/Srv/aic-route-options-diagnostic-20260920`. The release binary SHA-256 is
`96f6973087c5648eed9b415e8a95f5ac9b57774cf166590bb98d2c51f7710d94`.
The server workspace check passed, and nine focused constructive learner tests
passed, including active-dead-end ledger closure, self-roll-in occupancy,
conditional PPO/SIL likelihood, and downstream-quality ordering.

| Arm | Seed | Config SHA-256 | Metrics SHA-256 | Report SHA-256 | Learner SHA-256 |
| --- | ---: | --- | --- | --- | --- |
| Control | 101 | `d51e10e5019cd306e29e51604daa4d1583fc598cfb0a3d93a8c5eb66ce228bc1` | `0363b15349c7253298dc585bf954e2dc81ad324c0476979b08f2d025da0561fb` | `5ec36a5991c6a85d2c58ee074bf66ff146c2508959a0f73bdc8a19a4f448e1cf` | `52b85a162014131e47e57e1e9b27540b99656e1b2e81e20e980fb1c2e68b5fcc` |
| Control | 202 | `ec25191f8ea4f2ed741e14c3abf815f22f7d3f0869f6c6bcc36e14c444157e99` | `441575fb055ac35a49e07179ae28d5d6619b4c163ec0e9494489435db271fca7` | `c24511f801f2f2051aa8bfd4e4ecc94d7eaad4b00905cd5f5220455b7644c62c` | `ef4598b5cd4e1fc463a5eb8750b6473648a8a1128c20996e0ba41853675b63ec` |
| Control | 303 | `061dfdcdc54467629c1f4f340040088549a7d8c21c34bc07412dd2d5f5ad0735` | `b4ff6d092f4ce78023392045955fa29ae52a93001b04e220f6fd91ef5d6bb578` | `d83e571fac8fc175fbf9934b3e794bbdad318dc3fce0146b31c050864518b585` | `a93b537d0dd894527bfe61354adc1d1f93a6cbb929e6af51b349605a8b3b10d0` |
| Replay | 101 | `94ad9eda6e83bbf25a2ceb106859aacb6998fb802d21a0d1af8a5ee8d3b08cd8` | `e01489f949f5417e7b5b263c44087c8b342e540b2154085d5af091928767679f` | `edde0f5a556d904133eff4650e1aa3aec9276412d5652fdcd6341e36e16672a3` | `c3914f01d8436a7dc924c062b606c86ce85d6fcc7973eda1f650e1388efd15e1` |
| Replay | 202 | `672a2a7bdb6fc1cd307a4b94a29d2ac90aa1f68f4f9ee786e2ba840ea088fc18` | `f9f5e645d8c3d76cff64883aa8f7b0a2b4260f52cbb3c4becb1989356358f818` | `3cf86fd2dc06e726a319e66c4789c57764e2504eda59eac5ebfc64b00496d11a` | `dcf754d1df844b620fcd3716159ad6abacafadce03acfd21ef7359a1225562a1` |
| Replay | 303 | `7401629034d5dd058fb01a9451ed866c3b930aa3519b66878d3178a777be68b4` | `68aa166819dd247d4d3b97cfe7e89d7a95bd838491e0ae0647cba30cdc19f1af` | `67208a31df89b37ba3544d28abc5be3d5e387cef328f6062eba8fdd368e8f623` | `3c13a9ae2e75b9300f9e15e5dd633ba3f1a99a4fae499e2f940ce002e3ea8916` |
