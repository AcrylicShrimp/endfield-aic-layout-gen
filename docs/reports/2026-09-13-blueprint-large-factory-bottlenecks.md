# Large-factory learning bottlenecks in the fresh-input advantage screen

Status: retained-evidence diagnosis complete. No additional training was run.

## Decision

Use centered-only advantages as the working baseline for the next research
slice, while retaining the [primary comparison's](2026-09-12-blueprint-fresh-input-advantage.md)
geometry-overlap caveat. Do not infer that it already solves large-factory
editing. Before a long training run, investigate and establish a learning signal
for the large-factory move/repair/commit sequence.

The positive aggregate learning trend comes almost entirely from factories with
two or three facilities. The 16--26-facility factories do not show the same
trend. Centering replaces near-inactivity there with extensive unfinished
movement. Longer training would keep collecting 64-action episodes with the
same candidate set; it does not itself give each factory more editing steps.

## Scope and continuity

This is an explicitly invoked, one-off read-only analysis of the completed
six-arm matrix from source `f313821`. It reads 192 unique final evaluation traces,
six unique final success-buffer checkpoints, and existing JSON evaluation/update
reports. A targeted follow-up reads the recorded labels of the five rare large
training successes. It performs no new policy actions, native session execution,
optimizer replay, model inference, or complete-run proof audit.

Inputs are the retained completed matrix, its final case/checkpoint references,
and the actual training-input episode metadata. Output is
`blueprint-retained-bottleneck-analysis-v1`: per-case trace references, fixed
geometry bounds, behavior counts, case-size groups, successive block means,
within-episode milestones, replay coverage and recorded KL stopping. Missing or
changed referenced compressed inputs fail the existing input/hash check. Output
must be a fresh path. The reproduction script is scoped to this registered
six-arm, eight-stream experiment rather than a general training command.

The accepted constraints remain intact: fresh product/rate/geometry inputs,
original directed topology and endpoints, validated retained best, area-first
scoring, SIL coefficient .05, and existing episode/chunk boundaries. The earlier
smoke candidate restriction and fixed-anchor restriction are documented design
choices in the [blueprint contract](../designs/2026-09-11-blueprint-edit-contract.md),
not newly discovered game rules. No unapproved teacher, rewiring, changed reward,
or smaller training distribution was introduced here.

## The aggregate trend hides two populations

The fixed evaluation cohort contains 15 factories with two or three facilities
and 17 with 16--26. The following are three-seed means under centered advantages.
Each block contains 32 evaluations of the same factories.

| Updates | Small-factory area reduction | Large-factory area reduction |
| --- | ---: | ---: |
| 1--128 | 1.33664% | 0.00085% |
| 129--256 | 2.00407% | 0.00000% |
| 257--384 | 2.99107% | 0.00000% |
| 385--512 | 3.31924% | 0.00000% |

For both advantage modes, all 1,632 large-factory evaluations in the final block
have zero area improvement. This is 17 factories x 32 checkpoints x three seeds
per mode. There were a few earlier large-factory improvements, so the full run
must not be described as literally zero at every checkpoint.

Final small-factory means are 2.00802% standardized and 4.36624% centered; both
large-factory final means are zero. The final small-factory sample comprises 45
seed/case executions and the large sample 51. These are fixed repeated cases,
not independent samples of arbitrary factories.

## Centered policies move but do not close large drafts

| Final large-factory trace metric | Standardize | Center |
| --- | ---: | ---: |
| Policy actions | 3,264 | 3,264 |
| Pose actions | 74 (2.27%) | 2,787 (85.39%) |
| Commit attempts | 1,356 (41.54%) | 19 (0.58%) |
| Physically valid commits | 1,211 | 0 |
| Executions with any valid commit | 51/51 | 0/51 |
| Unfinished final draft | 18/51 | 51/51 |
| Mean final pending connections | 0.27 | 42.90 |

Every centered final large draft has both pending connections and facility or
component conflicts. All 19 attempted commits fail with
`blueprint.unfinished-conflicts`. The policy attempts 445 individual repairs,
of which 93 reduce the pending count, and only 13 repair-all actions; all 13
repair-all actions fail routing. Initial valid layouts remain the exported best,
so these failed private drafts do not imply invalid factory output.

Standardization's valid commits are not evidence of useful editing: none reduces
area, and 1,201 of its 1,334 repair-all attempts begin with no pending connection.
The two modes exhibit different unsuccessful behavior on large factories:
repeatedly confirming an almost unchanged layout versus accumulating unfinished
edits. Movement probability alone concealed that distinction.

The current candidate generator offers local translations by one or two tiles,
other facilities' current origins, individual repairs, repair-all, commit and
restore-best. Sampled poses retain the current rotation. Native manual actions
support rotations, but this policy does not propose them.

Of centered large-factory poses, 1,637/2,787 (58.74%) have Manhattan displacement
greater than two. Given this exact candidate generator, those destinations must
be another facility's current origin, which entails overlap if accepted. This
is a source-based inference from the recorded action distances, not a new
collision replay. Overlap is intentionally permitted for multi-action editing;
the problem is failure to resolve it afterward, not the mere existence of such
candidates. The data do not yet distinguish inadequate action selection,
state representation, credit assignment and learned phase sequencing causally.

## SIL has no centered large-factory successes to imitate

All three centered runs begin with empty success buffers and collect their own
successes. Final positive-area commit coverage is:

| Training product | Generated episodes per seed | Standardize, three seeds | Center, three seeds |
| --- | ---: | ---: | ---: |
| Watered grass | 533 | 246 | 423 |
| Copper liquid | 518 | 343 | 381 |
| Food | 483 | 4 | 0 |
| Medical | 514 | 1 | 0 |
| Total | 2,048 | 594 | 804 |

Each centered learner consumed 997 medical/food episodes, so the three learners
executed 2,991 such episodes without a positive-area commit. All 804 centered
success-buffer entries come from the two small product families. The counts
were reconstructed from final buffer entries and occurrences and matched each
checkpoint's cumulative positive-commit count; episode slots resolve to the
actual shared input metadata.

Increasing the SIL coefficient alone cannot create missing large-factory
demonstrations. It would increase the weight of the existing success population,
which could still transfer useful behavior, but such transfer is not established
by the current large-factory evaluations. The coefficient .05 is not by itself
a measure of gradient influence, especially after changing PPO advantage scale.
SIL uses positive return minus predicted value and only archives positive-area
commits; an accepted wire-only score improvement has zero area reward.

## Some large successes already fit the existing episode and action contract

The five standardized large training successes use one changed pose label,
one changed repair-all label and one positive commit label each. Their complete
recorded success segments contain 3--12 actions; excluded labels are not a claim
that deleting intermediate actions has been replay-verified.

| Arm | Input slot | Product | Facilities | Recorded positive commit step | Area reduction |
| --- | ---: | --- | ---: | ---: | ---: |
| Standardize 17 | 152 | Food | 23 | 11 | 1.42857% |
| Standardize 17 | 1,665 | Food | 16 | 38 | 1.58730% |
| Standardize 17 | 1,666 | Food | 18 | 42 | 2.35294% |
| Standardize 29 | 509 | Medical | 26 | 57 | 1.03093% |
| Standardize 29 | 1,179 | Food | 16 | 38 | 1.56250% |

For the 26-facility medical factory, the recorded actions at steps 55, 56 and 57
are pose, repair-all and commit, changing the validated area from 9,409 to 9,312.
All five commit samples retain their native validated witnesses. This establishes
that at least some large-factory positive edits are discovered within the current
64-action episode. It does not establish feasibility or sufficient horizon for
every historical evaluation factory. These are training examples, not evaluation
examples supplied back into training.

These five replay entries were sampled 975, 44, 38, 1,144 and 395 times respectively.
Rare success discovery alone therefore does not guarantee broad transfer. A
small reproduction/fit diagnostic can use them to distinguish an inability to
learn their action sequence from insufficient discovery or poor generalization.

## What the other evidence does and does not rule out

- **Universal lack of area opportunity:** not supported by the simple bound.
  Under the current frozen-anchor contract, non-facility anchors and logistics
  components keep their positions. Their inclusive bounding-box area is a lower
  bound on every complete layout. Only case 232700027 already equals that bound
  (5,520 tiles), so its area reward is structurally unavailable under this
  contract. Slack in the other 31 bounds does not prove a smaller layout exists.
- **Elapsed truncation:** excluded as the cause of missing actions in this run.
  Every recorded evaluation reached 64 policy actions. That says nothing about
  whether 64 actions suffice for a useful multi-facility edit.
- **Only more within-episode steps are needed:** unresolved, and too broad as an
  explanation. Centered small-factory last-block means at steps 8/32/64 are
  .73630/2.90284/3.31924%; extra steps still help those cases. Large-factory means
  are zero at all three milestones, and short large successes already exist.
- **PPO actor updates are universally shut off:** not observed. Centered final
  128-update blocks average 3.77/3.36/3.87 active actor epochs out of four by seed;
  early stopping occurs in 17/39/8 batches. Seed 29 has more KL stops, but that
  does not explain the shared large-factory failure across all seeds.
- **Critic, capacity, reward or SIL weight is the single cause:** unproven. The
  earlier critic errors mix policy-dependent state/return distributions, and
  this analysis does not measure separate loss-gradient contributions or run
  causal ablations. The preceding fixed-fixture capacity screen cannot settle
  capacity on these fresh large factories.

## Recommended next decision

Prioritize a short diagnostic around the existing rare large successes and
matched failures. Verify whether the current model can learn and execute their
useful pose -> repair -> commit sequence in a clearly labeled diagnostic fit;
retain original topology, rates, validity and the real action candidates. Keep
the authentic evaluation cohort out of the fitting data.

If that succeeds, investigate discovery and transfer: large-factory success
coverage, relevant facility/action selection, and whether replay's example
composition teaches completion on independent large inputs. If it fails even
on those examples, inspect representation, loss ownership and sequence learning
before increasing the total training budget. Do not assume adding an automatic
repair/commit wrapper or masking difficult actions is semantically neutral.

A longer-horizon probe is a separate question from longer training. Changing
the episode limit also changes normalized progress features in the current
observation, so a 64-vs-256 inference comparison is not a pure identical-input
continuation. Define that diagnostic explicitly if it is needed.

Only after seeing meaningful large-factory success should a longer training
run be the main next investment. Track product/size-stratified valid improvement
and completed edits, not only the 32-case aggregate or pose frequency. Preserve
the documented worker-migration and costly-default-audit obligations before
claiming the normal long-run path is ready. No additional training or input-contract change was performed in this diagnosis.

## Reproduction and evidence

Copy the committed reproduction script into the frozen source root and run:

```sh
"$PILOT_PYTHON" tools/factory-edit-pilot/analyze_blueprint_bottlenecks.py \
  --matrix artifacts/factory-edit-pilot/advantage-main \
  --output artifacts/factory-edit-pilot/bottlenecks-20260913/analysis.json
```

The original executed copy is retained at
`artifacts/factory-edit-pilot/bottlenecks-20260913/analyze.py` on the server and
is byte-identical to the committed script. The local evidence directory is
`artifacts/factory-edit-pilot/blueprint-bottlenecks-20260913`. The compact
[provenance record](2026-09-13-blueprint-large-factory-bottlenecks-provenance.json)
binds the script, analysis, arithmetic summary and five-success extraction.
The latter follows each final buffer entry's `(stream, episode)` to its input
slot, selects medical/food entries, and reads only the referenced label samples
from their original chunks. Raw inputs/checkpoints remain under the preceding
experiment's documented retention arrangement.

The actual diagnostic CLI completed all six arms. Its read-only assertions
checked shared initial geometry, selected input identities and reconstructed
success-count totals. No new broad Python test suite or training/replay run was
added. Required cached Rust workspace check/tests and artifact/index policy
checks accompany this analysis-source/report work unit.
