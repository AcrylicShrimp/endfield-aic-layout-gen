# Hierarchical policy stability: independent-stream confirmation

Status: protocol and decision thresholds fixed before confirmation training.
This follows the closed one-stream
[factorial screen](../reports/2026-09-23-hierarchical-policy-stability-factorial-result.md).
That screen showed a replay advantage in both failed-target settings, while D's
aggregate 25→27→31→34 rise still replaced solved cases. It did not select a
production learner.

## Question and fixed treatments

Repeat the same A/B/C/D two-factor comparison on three **new** training-panel
streams. A and C have no replay; B and D replay validated successful outer
roots from the prior four generations. A and B train failed-root policy
targets; C and D omit only those policy terms. Failed-root value targets remain
in every arm. All 12 runs start from the same stopped-run generation-87 weights
and keep lower training frozen. Reuse the screen's eager search, Gumbel seed,
200 epochs, 256 outer root-state slots, 16-state batches, 3,200 outer updates,
32 training cases per generation, two-facility level and eight-generation
duration. Do not tune any setting using the confirmation outcomes.

The panel streams are fixed before training:

| Stream | Training-panel seed for generation 0 | Namespace stem |
| --- | ---: | --- |
| S1 | 20261010 | `hierarchical-policy-confirmation-s1-v1` |
| S2 | 20261030 | `hierarchical-policy-confirmation-s2-v1` |
| S3 | 20261050 | `hierarchical-policy-confirmation-s3-v1` |

Within a stream, generation `g` uses seed `base + g` and namespace
`stem-g{g:06}`. All four arms of that stream use byte-identical panels. The
generation-training seed remains `20260925 + g` in all streams and arms, so
only training examples vary across streams. Finish all 12 arms unless a
correctness, resource or tracking guard fails.

## Diagnostic separation

Prepare 512 train-source diagnostic candidates with seed `20261100`, namespace
`hierarchical-policy-confirmation-diagnostic-v1` and the same two-facility
bounds. Select the first 64 distinct candidate problem identities absent from
all 24 predeclared training panels **and** the previous screen's diagnostic
panel. This gives one fixed new diagnostic panel across all three streams and
four arms. The prior panel must be deterministically reconstructed from its
tracked seed, panels and selector, and its SHA-256 must match the closed smoke
evidence before using it as an exclusion input. Keep the former 128-case
validation panel untouched. Evaluate this new panel after generations 1, 3, 5
and 7. Evaluate the common starting weights once on the new panel before
training. Audit all panel problem hashes before launch. No diagnostic outcome
affects training or promotion.

## Measurements and thresholds

Use the same compact analysis as the screen: paired case gains/losses, last-two
mean and minimum, at-most-four-connection easy stratum, connection strata, final
transition retention, failure terminations, replay slots, successful/failed
policy roots, unvisited target mass, model movement and integrated wall time.
Report every stream separately before pooling. A stream is an independent unit
for interpreting treatment reproducibility; 64 cases from one panel are not
64 independent training replications.

A replay arm can advance to an integrated lower-training check only when all
of these predeclared conditions hold across the three **new** streams:

1. Its final-two mean exceeds its matched no-replay arm by at least 4 of 64 on
   the three-stream average, with a positive difference in at least two of the
   three streams. B compares with A; D compares with C.
2. Its final-two minimum exceeds the matched no-replay arm by at least 3 of 64
   on the three-stream average. This prevents a single high final checkpoint
   from carrying the result.
3. In at least two streams, its final at-most-four-connection completion is no more
   than one case below that stream's common-start score, and its last
   evaluation loses no more than one third of cases solved at generation 5.

Four cases (6.25 percentage points) is below both screen replay contrasts
(+11 and +5 on the final-two mean) but larger than a one- or two-case swing.
The one-third retention bound separates the screen's replay arms (B lost
11/33; D lost 9/31) from its no-replay arms (A lost 13/24; C lost 11/26).
These thresholds are confirmation gates, not statistical confidence claims.
The fixed diagnostic selection has only one three-connection case but 14
at-most-four-connection cases, so the latter is the easy stratum for this
confirmation. This definition was fixed before the common-start evaluation
or any confirmation training.

If both replay arms pass, choose the one with the higher three-stream average
of its final-two **minimum**. Prefer B when that difference is less than three
cases because B keeps the existing failed-target rule; prefer D only if it
exceeds B by at least three cases and meets the same easy-stratum guard. D's
monotone aggregate pattern is also recorded per stream, but is not used to
select a model because individual case retention is the stronger requirement.
If neither passes, stop and investigate update magnitude rather than adding
another factor to this confirmation.

Passing confirmation authorizes only a bounded longer run with lower training
restored and an untouched final held-out panel. The integrated run must keep
the selected replay/target rule, compare against the same-start control, and
retain resource, recovery and physical-validity contracts. It does not
authorize an ultra-long launch. A final-panel threshold must be fixed before
the integrated run's outcomes are examined.

## Continuity and runtime

The expected 12 arms take about 5.6 GPU hours from the measured 27-29 minutes
per arm, plus setup and W&B finalization. Run sequentially on `ashrimp-arch`
under `~/Srv`, with the existing nonblocking resumable W&B observer. Verify a
real acknowledged record from the closed screen before the long launch, then
verify each arm's observer readiness. Check disk and active GPU processes
before starting; stop on unequal panel hashes, update budgets, lower checkpoint
movement, missing replay, invalid publication or W&B failure. Keep the normal
in-memory collection handoff and existing atomic recovery formats. A short
configuration/identity check suffices before the expensive matrix; the closed
A/D training smoke already validated the changed learner controls.

Close out the complete confirmation before any integrated run: commit the
exact source and configuration identity, per-stream compact metrics and
case-level evidence, verdict, reproduction commands and cleanup manifest, then
remove regenerable raw collections, panels, optimizer checkpoints and build
caches. Keep only inputs with a concrete continuation requirement.
