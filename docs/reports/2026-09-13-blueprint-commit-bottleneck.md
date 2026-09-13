# Missed commits are real, but rarely explain fresh-transfer failure

Commit timing is a confirmed weakness. In the retained final transfer population,
however, only **2 of 446 failed episode executions** visited a physically valid,
area-improving draft that could have been saved by one commit. The other 444
failures did not visit such a pre-action state. This locates most observed
fresh-transfer failures upstream of a useful commit opportunity.

Conditional behavior is substantially weaker: when B50 starts at the five known
successful pre-commit states, 44 of 120 branches fail to save any improvement,
although an immediate native commit succeeds from every starting state.
These populations answer different questions; the supplied states must not be
used to estimate commit-opportunity frequency on fresh factories.

See the [screen results](2026-09-13-blueprint-replay-balance-results.md),
[continuity checklist](../todos/2026-09-13-blueprint-commit-bottleneck.md), and
[compact provenance](2026-09-13-blueprint-commit-provenance.json).

## Scope and meaning of an opportunity

The explicit isolated command reads 480 endpoint transfer traces: five arms,
three learner seeds, sixteen factories and two sampling streams. The registered
fifteen-factory subset contains 450 episode executions and 28,800 pre-action
observations. The single excluded geometry and original sampling streams are
unchanged. The separate conditional analysis reads 120 B50 pre-commit branches.
No model inference, policy rollout, training, generator or optimizer executes.

A useful opportunity is an actually visited pre-action draft for which a commit
can pass the original native physical validation and reduce area relative to the
then-current retained best. There must still be an original action slot available.
Zero pending connections and zero conflicts only screen candidates; they are not
assumed sufficient for validity. A facility-body bounding-box lower bound safely
excludes candidates that cannot reduce area, though no such exclusion was needed
in this run. Native validity/score results are cached by blueprint and exact
facility/path geometry and compared with each state's actual retained best.

Existing accepted commit metrics and witnesses are reused. Other candidate
states use the existing observation-to-draft helper and native checkpoint
restoration, preserving original best, identities, topology, rates, revision,
attempts, action budget and accumulated elapsed time. Semantic identity is checked
before one commit on a private copy. No repair or pose is inserted. Five further
one-commit proofs verify the imported pre-commit starts and demonstrated areas.
Counterfactual witnesses pass the existing directed-topology audit.

Commit is not episode termination. Failure means no improvement in retained best;
an unfinished final draft after an earlier useful commit is not classified as
failure. Opportunities after the last permitted action are not actionable within
the original episode and are outside the pre-action census.

## Primary transfer findings

Each arm below pools ninety episode executions across its three seeds. These are
repeated factories and sampling streams, not ninety independent factories.

| Arm | Improved episodes | Failed episodes | Failures with useful commit available | Failures without useful commit available |
| --- | ---: | ---: | ---: | ---: |
| C | 0 | 90 | 0 | 90 |
| U05 | 0 | 90 | 0 | 90 |
| B05 | 0 | 90 | 1 | 89 |
| U50 | 0 | 90 | 0 | 90 |
| B50 | 4 | 86 | 1 | 85 |
| Total | 4 | 446 | 2 | 444 |

Only six episodes ever visit a useful opportunity: the four actual improved
episodes and the two missed failures. Five skip at least one opportunity; three
of those still achieve an improvement later or had already saved one earlier.
There are eleven consecutive opportunity windows and fifteen useful pre-action
observations. The policy chooses commit at four, pose at seven, and repair-all
at four. These observations are correlated; repeating repair-all four times at
one unchanged draft does not create four independent missed factories.

All 1,023 surface-complete pre-action observations in the strict subset are
physically valid by cached native evidence. Only fifteen improve on current best.
Thus a rule that merely commits whenever routing is complete would mostly save
unchanged or worse-area drafts in these traces.

| Arm | Commit attempts | Valid, no area gain | Positive-area commits | Rejected for overlap | Rejected for pending edges |
| --- | ---: | ---: | ---: | ---: | ---: |
| C | 12 | 0 | 0 | 12 | 0 |
| U05 | 43 | 0 | 0 | 42 | 1 |
| B05 | 174 | 106 | 0 | 37 | 31 |
| U50 | 59 | 18 | 0 | 39 | 2 |
| B50 | 297 | 207 | 4 | 20 | 66 |
| Total | 585 | 331 | 4 | 150 | 100 |

One valid non-area commit improves a later score component, so native status
`improved` occurs five times but positive area reward only four times. All 250
rejections are existing overlap or pending-edge diagnostics; none is a failure
of full validation from a surface-complete draft in this sample. Increasing raw
commit frequency would not resolve those unfinished drafts.

Including the excluded sixteenth factory gives 480 episodes, four improvements,
two missed failures and 474 failures without a useful opportunity. The main
interpretation is unchanged. Full per-seed totals are retained in the raw report.

## Two concrete missed failures

**B50, seed 29, food factory 401000024, sampling stream 0.** Step 2 moves a
facility; step 3 repairs all paths. Before step 4, a native commit validates an
area reduction from 3,526 to 3,444. The policy moves another facility instead,
detaching two connections. Step 5 repairs them and presents another valid
3,444-area opportunity. Step 6 moves again, leaving four pending connections and
one conflict. The branch ultimately retains the original 3,526-area best.

**B05, seed 41, medical factory 401000064, sampling stream 1.** Step 23 attempts
commit too early and is rejected with two pending connections. Step 24 repairs
them, yielding a fully valid draft with area 2,745 instead of 2,790. At steps
25–28 the policy repeats repair-all despite zero pending connections. At step
29 it moves another facility, leaving seven pending connections and two conflicts.
It never saves the smaller draft. This is direct evidence of incorrect timing:
commit before repair, then redundant repair after completion, then destructive
movement relative to the available improvement.

Two successful B50 episodes also leave additional area gains unclaimed: seeds
17 and 41, factory 401000056, sampling stream 1, save area 3,410 but later visit
valid area 3,355 drafts and move instead of committing. There are therefore four
episodes with unclaimed area in total, only two of which are outright failures.

## Known pre-commit starts

All five exact imported pre-commit states pass an immediate native commit, attain
their original demonstrated area and retain the original clock/action limits.
Each learner seed has forty B50 branches from these starts.

| Seed | Immediate first action is commit | Eventually improves | Attains demonstrated target | Fails to improve | Failed branch's first action |
| --- | ---: | ---: | ---: | ---: | --- |
| 17 | 16 | 26 | 24 | 14 | 13 pose, 1 repair-all |
| 29 | 18 | 26 | 24 | 14 | 12 pose, 2 repair-all |
| 41 | 15 | 24 | 23 | 16 | 14 pose, 2 repair-all |
| Total | 49 | 76 | 71 | 44 | 39 pose, 5 repair-all |

The five-action proof establishes availability; the recorded branches establish
what the policy actually did. The 44 failures could have saved an improvement
at their first decision. A first repair-all does not itself prove destruction;
it delays commitment and these five branches subsequently fail. This conditional
36.7% failure rate confirms that commit selection remains unreliable even on
supplied states. It does not overturn the sparse opportunity census on fresh inputs.

## Interpretation and next bounded question

The evidence supports a specific commit-phase defect, including both premature
attempts and missed useful commits. It does not support commit omission as the
main explanation for the recorded fresh-transfer failures: 444/446, or 99.55%,
have no useful visited pre-action commit opportunity.

That percentage is descriptive, not an upper bound on the benefit of changing
the policy. Inserting a commit changes subsequent best-state observations,
rewards, behavior and learning. The census does not predict those new trajectories
or show that the policy's other poses could never lead to a useful layout.

The next diagnostic should connect useful movement, successful repair and commit
timing, with explicit tracking of when an opportunity is created or destroyed.
The medical example particularly motivates measuring action-kind selection
before and after repair completion. Any later learning change still needs fresh
episode inputs and independent transfer/preservation evaluation. No automatic
commit wrapper, mask, longer horizon or training treatment was introduced here.

## Execution, verification and limitations

The command completed in 5.06 seconds using eight worker processes on distinct
cores 0–7. It needed 36 native transfer-draft commits plus five anchor proofs,
well below the explicit 2,048-probe and 600-second limits. It authenticated only
selected evidence, loaded runtime modules, the native binary and catalogs at the
diagnostic boundary. No expensive work was attached to normal training.

Original transfer trace hashes are bound by retained endpoint reports. Historical
anchor reports bind complete branch result rows but did not bind full trace-file
hashes. This diagnosis checks each trace's complete row against its bound report
and records its current hash; historical first-action integrity remains weaker
than transfer-trace integrity. The availability proofs independently authenticate
the admitted original pre-commit states.

The actual CLI verified observation reconstruction, native candidate commits,
selected source identity, complete 480/120 coverage and valid historical results.
Two targeted tests cover safe translated footprint bounds and physical cache
identity. `cargo check --workspace` and `cargo test --workspace` passed (564
passed, five ignored). Worktree/index artifact checks accompany this slice.
No production or training runtime module changed.

From the frozen replay root, using the existing Torch environment:

```sh
"$PILOT_PYTHON" artifacts/factory-edit-pilot/commit-diagnosis-code/diagnose_blueprint_commit.py \
  --matrix artifacts/factory-edit-pilot/replay-main \
  --provenance artifacts/factory-edit-pilot/commit-diagnosis-code/2026-09-13-blueprint-replay-results-provenance.json \
  --output artifacts/factory-edit-pilot/commit-diagnosis-v1 \
  --workers 8 --seconds 600 --max-probes 2048
```

The output path must be fresh. Retain the frozen source, original evidence and
the small per-case/proof reports under ignored artifacts. No raw trace is added
to Git and no new factory or policy trajectory is generated.
