# Training setup verdict after bounded retention experiments

**Verdict: keep the current baseline.** This is the best-supported operational
choice from this experiment chain, not a demonstrated globally optimal learner.
Main training remains **paused at update 1744**. It was not resumed, extended or
switched to an experimental checkpoint.

## Selected settings and evidence

Keep v1 observed-contribution retention, balanced-available uniform replay with
eight sequences per minibatch, SIL coefficient 0.05, capacity 4,096 sequences /
262,144 labels, EMA alpha 0.5, maturity after eight draws in two updates, and
staleness scale 64 updates. Keep the current optimizer, native limits, generated
training inputs, curriculum and evaluation panel.

| Applicable experiment | Actual result | Meaning for the decision |
| --- | --- | --- |
| Indexed category retention verification | Passed actual-capacity and unchanged-default transition checks | An opt-in implementation exists; no live adoption |
| Complete baseline continuation | 104 updates, 14,735 terminal attempts, 14,729 distinct new IDs | Sufficient actual chronology for the registered screen |
| Replay sampler verification | Counts 8/16/32 and score/positive-advantage references verified; default transition equal | The tested alternatives preserve the default path when disabled |
| Complete native geometry audit | Nine native replays; both reversal families decisively covered; no extra loop bonus | Geometry package eligible for a limited screen, not proven useful |
| Frozen chronological screen | All 75 arms completed; `no_candidate` | No shortlisted transition or learning comparison is authorized by the reviewed gates |
| Isolated candidate readiness | `BLOCK`: no shortlist; runtime checks not executed | No candidate recovery, throughput or actual checkpoint-storage PASS is claimed |
| Paired candidate/baseline learning comparison | Not executed because the prerequisite returned no candidate | No trained-policy superiority or panel improvement claim |

The [screen report](2026-09-14-chronological-retention-screen.md) contains the full
methods, controls and limitations. Baseline median 32-update facility-edit exposure
was 93.75% large and 97.83% small. Alternatives can raise these to 100%, but small
exposure can improve by at most 2.17 percentage points. The preregistered rule
requires at least five points in both strata. This ceiling blocks every candidate;
it is not a finding that all alternatives are worse.

There are useful positive signals. Category retention preserved facility-edit
entries for all 32 follow-up updates, and positive-advantage sampling concentrated
more draws on positive frozen contributions. Those measurements cannot establish
policy-learning gains. Increasing replay to 32 also exceeded the 125% source-byte
gate in every profile, while all geometry packages failed their matched incremental
exposure gate. Selecting a new setup now would require overriding the reviewed
selection rule without the required learning/runtime evidence.

The defensible decision is therefore **baseline / no change**, with alternative
learning value unresolved. A future investigation would need a separately reviewed
scope and an informative metric under near-saturated at-least-once coverage.
No new threshold, candidate, run extension or automatic restart was introduced.

## Plan closure and verification

Plans 00, 01, 02, 03 and 05 are Completed. Plans
[04](../plans/2026-09-14.04-isolated-retention-readiness.md) and
[06](../plans/2026-09-14.06-training-setup-verdict.md) are Blocked by the explicit
no-shortlist branch of their reviewed proposals. Their runtime criteria remain
unchecked; the blocked/no-adoption outcome and final recommendation are recorded.
No unselected transition or comparison engine was implemented merely to bypass
the gate. All applicable experiments have ended.

Final server inspection verified the launch, matrix state, job state and parent
checkpoint against the pre-experiment bindings. Matrix and job are both paused,
job stop reason is `requested_stop`, and update is 1744. No duration runner,
arrival collector, screen process or declarative worker remains. Main training
configuration and checkpoint are unchanged. Generated diagnostic intermediates
from the failed native case-preparation attempt were removed after retaining the
finding; actual corpus, models, inputs and complete native evidence were preserved.

Relevant native Python tests and workspace verification passed as detailed in the
component reports. No candidate learning tests are represented as passing. This
closeout changes research documentation and optional diagnostics; it does not
change the paused server launch.

## Retained receipts

Server source root:
`/home/ashrimp/Srv/aic-retention-candidates-20260914`.

- `artifacts/retention-screen-complete/report.json`: complete frozen comparison,
  SHA-256 `092a6858569d6c96b36dfc71cb30aefd404375ee48dcb0103a364fbff0dd151c`.
- `artifacts/retention-readiness.json`: explicit no-candidate BLOCK receipt, bound
  to the same immutable parent and screen.
- `artifacts/training-setup-verdict.json`: baseline/no-change verdict, bound to
  both preceding receipts, selected settings and final paused-parent inspection.

Compact copies are retained locally under
`artifacts/retention-experiments/20260914/`. The frozen parent checkpoint SHA-256 is
`e09bce8cd3d32bab3ccfc3652825e263cc48f2a24ae396f041a0af6b85486cc4`;
launch SHA-256 is
`e5abc14224f7631f6125f520ccc2a2d97d8abd1e78f94daeeef79010fee0838d`.
Raw generated outputs remain ignored; concise findings, commands, configuration
and experiment code are committed.
