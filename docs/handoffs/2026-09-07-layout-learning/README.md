# Learned layout improver: paused handoff

Status: paused by the user, 2026-09-07. No training or diagnosis is running.
Read this file first when resuming; older experiment instructions to continue are historical.

## Mandatory restart posture: adversarial review before reuse

Do not trust the inherited technical direction by default. Treat pre-handoff designs,
explanations, success claims and proposed next steps as hypotheses to challenge, not
an established route to the user's goal. Existing code, accumulated experiments, prior
confidence and sunk effort are not reasons to preserve a method.

Before reusing an existing architecture, reward, action representation, sampling policy,
metric or experimental conclusion, subject it to an adversarial review against the
original user requirements and inspect the underlying evidence independently. Actively
seek counterexamples, objective mismatch, invalid assumptions, information leakage,
confounded comparisons, missing controls and untested scope. A prior "verified" label
or passing test is not sufficient: establish exactly what it checks and what it leaves
unproven. Do not carry earlier claims forward merely because this handoff repeats them.

Retain an approach only when that review gives a concrete, defensible reason to reuse
it. Otherwise recommend revision, replacement or retirement instead of optimizing the
old direction by inertia. The diagnostic suggestions later in this handoff are also
subject to this review; they are not a mandatory implementation plan.

This skepticism applies to inherited technical claims, not permission to override the
user's explicit constraints, destroy evidence or erase unrelated work. Preserve original
artifacts and distinguish raw observations from interpretations while reassessing them.
The user specifically requested this restart posture in addition to the pause.

## Latest user decisions

1. The user asked whether loss or outcomes demonstrate learning, rather than merely a live
   process. The fixed evaluation curve does not show sustained improvement from 64 to 256.
2. The user challenged the justification for four-trajectory relative-rank credit. The
   fixed-opponent gradient result is not a convergence or goal-achievement proof.
3. The user explicitly stopped the cumulative training study before 1024 and requested
   diagnosis of the plateau before more training. Do not describe the stop as numerical
   divergence, resource exhaustion, proof of impossibility, or a completed budget study.
4. The user then paused that diagnosis too, requesting a committed context-preserving
   handoff and an explanation of remaining uncommitted changes. Do not start a new probe,
   resume the old study, change reward/model, or delete those changes without new direction.
5. The user also questioned repository growth. A read-only
   [size investigation](repository-size.md) found verbose tracked benchmark artifacts
   dominate line count; ignored build/research files dominate disk usage. That initial
   inspection performed no cleanup. The exact 1.4-million-line UI comparison base was
   not established. The later cleanup decision below supersedes its retention advice.
6. The user explicitly instructed the next session not to trust the inherited direction
   and to review it aggressively before any reuse. Apply the mandatory posture above
   before adopting even the diagnostic suggestions in this handoff.
7. The user subsequently authorized reducing `target/`, discarding regenerable raw data,
   and keeping generated outputs out of Git by default. The
   [cleanup record](artifact-cleanup.md) documents removed dev builds and old constructor
   candidates. The paused learning study, checkpoints, final selected constructor
   witnesses, required inputs, and uncommitted source overlay were preserved.
8. The user then explicitly requested removing generated benchmark payloads from Git,
   keeping concise non-regenerable findings and rejecting oversized files even when
   they are non-regenerable. The [source-only cutover](../../maintenance/artifact-cutover.md)
   moved 2,246 raw files to ignored `artifacts/benchmarks/`, extracted independent test
   inputs and introduced a 100 KiB limit for every documentation/fixture file.

The overall product objective is unachieved, not abandoned or proven impossible. A goal
record that still says active or that the target needs clarification does not override
these latest decisions. Target identity was already resolved; do not ask it again.

## Actual objective and non-negotiable semantics

The target is the complete 59-facility Heavy Xiranite factory, item
`item-xiranite-enr-powder`, producing 0.1/s with the original water/sewage inputs and
low-polymer surplus export. The original request and full graph are preserved in the
existing evidence; do not substitute a smaller factory or external intermediates.

Required end state: learned improvement on diverse arbitrary factories, low futile-action
rates, at most 1% unused XY area for that target, and direct visual confirmation of dense
packing and non-gratuitous routing. Preserve complete independently valid factories and
lexicographic used area, physical transport tiles, directed turns. A valid temporary
deterioration may enable later improvement; do not prohibit it by an untested penalty.
No filler wiring, cropped requirements, lost incumbent, global exact-solver fallback,
or caller-supplied production width/height limit. The full acceptance contract is
[layout-production/contract.md](../../benchmarks/layout-production/contract.md).
Its 10% mean / 20% p95 futility limits and 300-request suite are working release gates;
the target's 1% and actual visual inspection are explicit user requirements.

The separate parallel-component constructor-admission proposal remains unapproved.
The measured connected medium factories do not resolve that policy issue or establish
held-out medium/large generalization.

## Stop state and preservation

The three verified training PIDs received SIGINT and all three tool sessions returned
exit code 130. A subsequent process-list check found no remaining matching learners or
environment workers. Do not reuse old PIDs as future kill targets: PIDs can be recycled.

| Seed | Last completed update | Actions in completed training groups | Evidence directory under target/ |
| --- | ---: | ---: | --- |
| 1901 | 399 | 817,152 | layout-direct-budget-seed-1901-02 |
| 2901 | 391 | 800,768 | layout-direct-budget-seed-2901-01 |
| 3901 | 389 | 796,672 | layout-direct-budget-seed-3901-01 |

The interrupted next groups and any partial files remain on disk but are excluded from
completed counts. No artifacts from these three training runs were deleted, including
during the subsequent constructor/build cleanup. There is no 1024 checkpoint,
1024 evaluation, final training report, or completed final budget curve.

Full model/Adam snapshots exist only at 0, 64 and 256. The last completed group numbers
are not saved final model snapshots. Capsules reproduce an optimizer update, not the
generator cursor, deduplication history and worker state needed for full-stream resume.
Do not restart from a 256 snapshot and call it an uninterrupted continuation.

[artifacts.json](artifacts.json) records exact checkpoint/evaluation/root hashes and paths.
At handoff, all 6,207 referenced completed-group root and episode files were hash-checked;
all group state chains and raw-versus-summary retained score/waste fields matched.
This covers 4,716 training plus 312 evaluation episodes and 2,734,080 proposals in those
completed records. Recorded full-factory validation flags passed. This is an integrity
check, not a fresh rerun of every geometry validator or gradient. The last independent
full semantic audit through update 256 is documented in the checkpoint report.

## What the learning curve actually says

Configuration stayed fixed: width 32, Adam lr 0.003, norm-1 clipping, entropy weight
0.0001, four independently sampled 512-action trajectories per update, full target
replay every eighth group, otherwise fresh admitted complete factories. Evaluation
uses 1024 proposals, twelve frozen development roots, two sampling seeds and three
training seeds, with a separate full-target result. No sealed release test was used.

| Checkpoint | Development wins / losses / ties versus initial | Mean win-minus-loss | Development futile actions | Target XY empty area |
| --- | ---: | ---: | ---: | ---: |
| 0 | Reference | 0 | 79.1273% | 29.6904% |
| 64 | 40 / 27 / 5 | 0.18056 | 79.3688% | 29.6904% |
| 256 | 38 / 31 / 3 | 0.09722 | 79.5844% | 29.6904% |

Direct 256-versus-64 comparison: 34 wins, 36 losses, two ties. The initial policy also
executes 1024 proposals; it is not the do-nothing root. These 72 comparisons are correlated
through twelve roots and three training seeds, not 72 independent learning replications.
The seed is the independent learning unit. Early gains are limited evidence, not a reliable
upward curve or arbitrary-factory generalization. More training being universally useless
does not follow either. There is no evidence that this is a normal, predictable warm-up.

All target evaluation tuples at 0/64/256 are `[2132,392,147]`, 41 by 52. Across all 580
target training episodes in completed groups at the stop, retained area also never
improved. Some training trajectories improve lower-priority routing, so do not claim
that every target score or every policy update is identical.

The loss question was checked through the common 320-update prefix, not to a selected
favorable endpoint. Pooled mean task losses for consecutive 64-update windows are
`0.008590, 0.014228, 0.010792, 0.012528, 0.019113`. No consistent decrease.
All 960 updates in that prefix had finite nonzero gradient norms, no active clipping,
changed model-plus-optimizer state fingerprints, and uninterrupted state chains.
These prove update activity, not useful representation learning. Mean peer-rank signals
are structurally zero; neither their mean nor policy loss is an absolute performance metric.
Fresh training factories differ across updates, so raw training quality averages are
confounded by difficulty. Do not replace the fixed evaluation curve with them.

## Existing evidence and unresolved diagnosis

- [256 checkpoint](../../benchmarks/layout-direct/budget-curve-256-report.md): full paired
  quality/work/attrition results. The fixed 14-facility visual worsens in area from
  `[1295,453,82]` at 64 to `[1435,439,76]` at 256 despite shorter wiring. Target geometry
  remains unchanged. Eight rendered frames were independently validated and directly
  inspected; 24 browser checks passed. The final 1024/post-hoc-worst visual never ran.
- [64 checkpoint](../../benchmarks/layout-direct/budget-curve-64-report.md): limited early
  benefit over initial and uniform controls, still high waste and poor target density.
- [Rank-objective limits](../../benchmarks/layout-direct/rank-objective-limits.md): exact
  fixed-opponent gradient interpretation, prefix-baseline assumptions, and a verified
  abstract counterexample where pairwise preference improves while expected area worsens.
  Expected area was not adopted as a replacement reward or release objective.
- [Update replay](../../benchmarks/layout-direct/update-replay-report.md): behavior,
  conditional decisions, model/Adam/RNG state and recorded updates reproduce. Five
  enumerated credit-reference tests were rerun successfully during the latest discussion.
  These finite checks do not prove universal correctness or convergence.
- [Gradient controls](../../benchmarks/layout-direct/gradient-audit-report.md): entropy
  dominance was not supported in the two recorded smoke updates; zero current signal can
  still move weights through Adam history. Do not generalize this to every later update.
- [Frozen-batch pilot](../../benchmarks/layout-direct/frozen-batch-report.md): sixteen
  independent single-update trials, not a cumulative run. Target prefix outcomes tie
  no-update controls on all 32 pairs. Gradient dispersion is substantial; prefix baseline
  helps observed dispersion on the target but worsens it on the small root. Cyclic peer
  reassignment is not an independent noise control. These results do not establish
  inadequate model capacity or select an alternative reward.
- [Forensic audit](../../benchmarks/layout-direct/trajectory-audit.md): prior coarse-credit,
  limited history observation and near-uniform kind-choice findings are scoped to that
  older run. They cannot be presented as fresh 0/64/256 same-observation measurements.

Still unresolved: whether the current policy can quickly fit known useful choices; whether
useful packing outcomes are discoverable often enough; whether credit points toward those
outcomes; whether parameter sharing/observation aliasing causes interference; and how much
additional training would help. The plateau is a reason to investigate, not a diagnosis.
The theoretical rank counterexample is not evidence that its specific failure occurred here.

## Next work, only after the user resumes

1. Apply the adversarial-review requirement above to this handoff, the rank limits and
   the forensic/frozen-batch reports. Revalidate local artifact and remaining-source
   hashes before relying on them. Do not rerun the budget study.
2. Start with existing data: compare 0/64/256 models on identical recorded observations
   across all decoder stages, with per-stage legal support. Separate parameter movement,
   policy-distribution change, chosen-action quality and target area signal. This probe was
   discussed but NOT executed before the handoff pause.
3. Review the deferred `choice-fit-contract.md` and `choice_fit.py` as unverified diagnostic
   drafts, not approved production code. A bounded known-choice fit could separate basic
   representation/optimization response from RL credit/exploration. It has NOT run and is
   NOT authorized to start merely by reading this document. Its two target labels improve
   ports, not target area; a pass cannot prove target packing capacity or generalization.
4. Register the precise distinguishing question, frozen inputs, controls, budgets and
   interpretation before any new diagnostic training. Do not jump straight to longer runs,
   a larger model, a density reward, or forbidding temporary deterioration.
5. If a future credit change is justified, define its objective and relation to unchanged
   score/void/waste requirements first, then measure a controlled main-path comparison.
   Do not report a toy fit as completion of the factory improver.

Suggested resume request: Read this handoff and the linked evidence. Keep the stopped
cumulative study stopped. Do not trust the inherited technical direction; critically
re-establish its assumptions and evidence before reusing any part of it. Diagnose the
64-to-256 plateau, distinguishing useful
signal, representation/optimization response and exploration. Do not implement a reward
or architecture change before the diagnosis supports it. Preserve uncommitted work.

## Files, environment and recovery warnings

- Workspace: `/Users/ashrimp/Devel/endfield-aic-layout-gen`; Rust stable virtual workspace.
  Repository files are English; conversation with the user is Korean.
- Python diagnostics use the existing environment, for example:
  `uv run --offline --frozen --project docs/benchmarks/layout-learning python -m unittest discover -s docs/benchmarks/layout-direct -p test_credit_reference.py -v`.
  System `python3` lacks Torch. The existing Torch warning about absent NumPy did not
  fail these tests; no dependency was added to silence it.
- `target/layout-direct-budget-evaluation-02/` holds the frozen evaluation set. The three
  training directories total about 9.2 GiB; preparation is about 55 MiB. These are ignored
  by Git and NOT included in the handoff commit. Keep them or separately back them up.
  Do not run `cargo clean`, delete `target/`, or move evidence without checking references.
  Older frozen-batch/replay evidence and rendered visuals are also under `target/` and
  remain local. A fresh clone alone does not contain the raw research evidence.
- Latest directly inspected cumulative visual:
  `target/layout-direct-budget-visual-256-01/cumulative-layouts.html`.
- [Remaining changes](remaining-worktree.md) inventories three modified tracked files
  and nine untracked files, including a constructor dependency used by the recorded runs.
  The committed patch is an archival overlay, not integration or approval of those drafts.
  All twelve reconstructed files were byte-identical in an isolated Git index.
- [Overlay manifest](remaining-worktree-manifest.json) pins the exact base commit, patch
  hash and original file hashes. Do not apply the patch over this already-dirty workspace.
- The artifact cutover moved the old `gap-1-placement.json` to
  `artifacts/benchmarks/constructive-chain-blocks/` and updated `sampling.json` to refer
  there. The study's recorded source-path/hash set therefore no longer matches the
  current checkout, although its checkpoints and episode bytes are unchanged. For an
  exact historical audit, restore the pre-cutover source and archived overlay with the
  original path layout. Do not rewrite historical provenance to bless new source bytes.

Important commits before this handoff: `31bfebc` cumulative runner; `37c0278` 64-update
report/visual; `424ac80` 256-update report/visual; `44a854f` rank-objective limits.
The original handoff commit intentionally did not incorporate the remaining source edits
as finished code. It performed no production cutover, new learning run or artifact deletion;
the subsequently authorized repository maintenance is documented above.
