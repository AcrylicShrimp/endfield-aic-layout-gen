# Cold-start quality replay comparison

Status: corrected execution running. The first attempt is invalidated; no final winner.

## Question and registered comparison

Does changing success replay sampling and retention improve early policy learning
when every arm begins with random weights and empty learned state?

The [accepted design](../designs/2026-09-14-cold-start-quality-replay-study.md)
registers eight profiles, 128 updates each under initialization seed 71, selection
of the two best nonbaseline development endpoints, and confirmation of those two
plus baseline. Confirmation combines read-only evaluation of their seed-71
128-update endpoints with three newly initialized seed-113 128-update runs.
The replacement allocates 1,408 scientific updates. An invalidated first cold
attempt consumed 128 additional updates, for a total ceiling of 1,536. The main
update-1908 run stays paused.

The former warm study is withdrawn as evidence for this question. None of its
learned state, priorities, input bank, panels, outcomes or shortlist initializes
or selects these arms. Numerical architecture and optimized execution code are
reused. The cold configuration is registered in
[`declarative-quality-study.json`](../../tools/factory-edit-pilot/operations/declarative-quality-study.json).

Both initializations pair actual fresh generated files by canonical slot, while
inherited starts come from each arm's own previous wave. Development and
confirmation each use 32 new geometries, 16 per size stratum, and two action seeds
per geometry. The replacement will exclude all five prior panels and reserve both new panels
before training admission. Policies apply from update 1 with no shared learned warmup.

Selection uses actual native development area reduction at update 128. It has no
coverage, saturation or minimum-gain gate. Confirmation remains inaccessible until
an immutable selection receipt exists. Final uncertainty uses 10,000 paired,
stratified whole-geometry resamples with both observed initializations attached;
it does not estimate broad initialization variance or convergence.

## First-attempt failure and correction

The first cold attempt stopped automatically after baseline update 128 and
1,484.149 seconds. Four development geometries (five training slots) and eight
confirmation geometries (22 slots) overlapped actual fresh training inputs.
Different generation seeds can produce identical physical geometry. Rate changes
are intentionally excluded from the established geometry identity. Every overlap
was a fresh start; no candidate was selected. Its quality curves are invalid as
held-out evidence and cannot select or initialize the replacement.

The corrected run reserves both new panels before any training admission. A
reserved native-valid generated candidate consumes an ordinary bounded attempt,
then retries before blueprint editing or policy actions. Actual inherited geometry
is checked too, including restoration, and fails closed on a collision. Donor
selection, quotas and fallback rules are unchanged. Existing closeout overlap
rejection remains. All five earlier panels are excluded by identity.

The first checkpoint and leakage closeout remain in the original frozen server
root. Its 1,315,338,556 retained artifact bytes are charged before the replacement
readiness or study writes. No learned state or old input bank is imported.

## Verified execution path

Initial cold implementation: `992cf74`. The input-separation correction passed
its second independent proposal review (overall round 4), PASS_WITH_NOTES. The cold launch validates a registered configuration family and source
receipt, publishes a complete empty initializer atomically, and rejects warm
receipts or banks. Normal packed CUDA training, persistent workers, compact
publication and checkpoint recovery remain in use.

- Corrected local and server checks: 39 passed, including fresh/inherited reservation rejection, restore, pairing, deadline and persisted cost contracts.
- Identical-input CUDA transitions: all eight profiles passed in 5.81 seconds,
  comparing ordinary, restored and observed restored updates with nonempty Adam.
- Integrated cold readiness: 76.067 seconds for five actual updates, comprising
  0→4 plus restoration of saved live update 1 and one valid update through 2.
- All 32 live streams restored their saved semantic/input state; elapsed native
  time stayed monotonic. Initial model, optimizer, RNG and empty buffer proofs,
  compact publication and all needed live/replay rows passed.
- Readiness used 133,185,230 actual unique added bytes. Its data and state are
  excluded from the scientific run. No capacity-fill or eviction gate was applied.
- Actual readiness rejected 15 reserved candidates across 78 generation attempts
  for 61 fresh slots, before admitting training inputs.
- Workspace check/tests and tracked-file hygiene passed before launch.

The CUDA receipt authenticates its unchanged execution/observer core. The cold
readiness receipt separately binds the complete current orchestration source,
native binary, numerical base and physical worker allocation.

## Results

The corrected execution has started from zero. Development ranking, confirmation estimates,
learning curves, quality allocation and mechanism activity will be reported only
from completed bound outputs. Missing data or an exhausted hard budget produces
incomplete evidence and no final winner.

## Runtime and retained evidence

The replacement is bounded by 18,000 seconds and the original six-hour absolute
deadline (Unix 1789374130.8190546), including setup, evaluation and finalization.
Its limits are 1,408 new updates, 12 GiB cumulative cold artifacts, 48 GiB
process-tree RSS and 16 GiB disk reserve. The 128 discarded updates and prior
artifact ownership are reported separately and retained in cumulative accounting. Successful last-link deletion releases owned storage charges. Command
entry and arm boundaries reconcile the ownership ledger; normal updates do not
scan the study tree or save an extra ledger.

Current frozen server root: `/home/ashrimp/Srv/aic-quality-cold-start-r2-20260914`.
Original invalidated root: `/home/ashrimp/Srv/aic-quality-cold-start-20260914`.
Its `artifacts/study/leakage-closeout.json` records the overlap and checkpoint.

- `artifacts/readiness/readiness.json`: bounded integrated readiness receipt.
- Original root `artifacts/cuda-proof.json`: reused unchanged-core CUDA evidence.
- `artifacts/study/study.json`: scientific allocation, source, panels and progress.
- `artifacts/study/selection.json`: immutable development selection, when available.
- Per-arm `quality-branch.json`, `quality-summary.json` and ordinary seed job state:
  initialization, measured outcomes/allocation and retained checkpoints.

Local readiness and CUDA receipts are retained under
`artifacts/cold-quality-study-r2-20260914/` and the original
`artifacts/cold-quality-study-20260914/`. Regenerable artifacts are ignored rather
than committed; these local/server directories are not backups.
