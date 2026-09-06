# Multi-step factory improvement RL experiment

Status: contract established before implementation, 2026-09-07.

## Question and authorization

The user requested a new goal: test CPU multi-step reinforcement learning that optimizes
the eventual returned layout, rather than only the next modification. They additionally
requested that outputs from both existing generators supply real starting factories.
This is isolated experimental tooling, not a production planner cutover or a global optimum
claim. A negative measured verdict is a valid experiment outcome.

The primary environment starts from complete independently validated generated factories:
the recent chain/fan-in constructor and replay variants, and the earlier complete mixed-kind
closure generator. Preserve each original request, exact inventory, recipes, utilizations,
material margins and actual external connections. An internal chain fragment cannot replace
this main experiment. Small controlled environments may separately test credit assignment.
Do not modify the paused breadth work or other pre-existing user changes.

## Inputs and import gate

An explicit schema-1 plan names catalog paths, validator, generated witness/report sources,
original graph dialect (`chain-factory` or `closure-factory`), seed/split policy, action and
resource budgets. All game values come from the catalogs on each execution. Resolve and hash
every imported witness, original request, catalog, executable and transitive implementation.
Recheck geometry and original-request coverage; never trust producer success flags. Record
failed imports and exact duplicate/rotation-equivalent roots, not just selected successes.

Use both earlier closure and newer chain-generated complete roots. Distinguish the earlier
large mixed-kind factory as a scale/family holdout if it is not included in training. Group
all transformed/augmented descendants of a root into one split; identical canonical geometry
cannot cross splits. The small existing recipe set does not establish unseen-topology breadth.
Preserve original unmodified roots and scores as baselines. Generated variants that share
construction history are correlated, not independent community designs.

## Transaction state and action contract

State owns a current private complete valid witness, an independently validated best complete
incumbent, the original root, remaining work/time and deterministic action-generation state.
Only a strictly improving validated candidate replaces the best incumbent. The private current
witness may become worse or equal and continue exploring. Invalid, unknown or late results do
not become states. Exhaustion always returns the validated incumbent.

The first main action family uses existing neutral local re-embedding: catalog-backed selected
facility/connected-pair translations and immediate repair of every affected route. Rotations,
port identities, flows and unrelated paths stay fixed according to that kernel's ownership
contract. Use pipe-first affected-chain order, terminal-flexible routing, and no new crossings.
Every primitive is a transaction; no all-facilities-first/global-deferred-routing construction.
An explicit event-free whole-grid strip compaction action is allowed, charged and validated;
it owns that global coordinate contraction and is not a free hidden postprocessing pass.

Translations initially use a declared symmetric finite step set, including moves with worse
geometric proxies. Immediate body-collision screening is allowed; score-improvement screening
is not. The inherited current-extent routing neighborhood is a local diagnostic restriction,
not a game W/H ceiling, production constraint or completeness-preserving action set. Report
when it blocks exploration. No larger movement domain is implied by a long-horizon reward.

Candidate repair uses one shared per-action route-expansion account, plus explicit validator
and episode caps. Charge failures, compaction attempts, considered baseline alternatives,
inference, IPC, validation and evidence work. A bounded action space can itself cause failure;
diagnose that separately from failure to learn its useful sequences.

## Objective and learning

Recompute every root/candidate with one common lexicographic objective: actual used bounding
area, physical belt/pipe cells, then turns including facility-port arms. For branch networks,
count directed incoming/outgoing perpendicular pairs, explicitly rather than silently reusing
incompatible historic turn metrics. Incumbent comparisons use exact integer tuples.

Train on complete trajectories, with no temporal discount that prefers early improvement.
The initial algorithm is Monte Carlo policy gradient with same-root groups: terminal returns
are exact lexicographic comparisons of the final best incumbents within the rollout group.
Ties have equal return and all actions in a trajectory receive its terminal advantage. This
avoids arbitrary area/tile/turn scalar weights and numerical loss of the lower-priority terms.
It optimizes a relative terminal-quality preference, not a calibrated absolute global value
or a proof of optimality. A changing on-policy comparison group is part of that objective.
No terminal outcome or action-validation result enters pre-action model features.

Include a matched myopic-credit control and an untrained/random-signal control, with explicit
seed/update/episode counts. Fit only on training roots; use validation for development and
freeze the final schedule before held-out evaluation. Report every seed and unsuccessful run.
The architecture may reuse the small PyTorch action-conditioned graph encoder; geometry,
connectivity, current/best score and remaining budget are legitimate observations. Document
lossy route summaries, feature bounds and action masks. No silent crop of a larger factory.

## Evaluation and CPU accounting

Compare retain-root, greedy actual-score improvement, nonlearning nonmonotone search and the
learned policies. Give them identical candidate/routing/validation caps, then separately compare
under real wall-clock deadlines. Greedy/beam evaluations of several alternatives are not free.
Use fresh routing/validation for timed evaluation; cached arithmetic cannot count as online
factory throughput. Charge training/environment startup separately and disclose amortization.

Report per-root initial/final tuples, improvements, valid transitions, rejected/unknown reasons,
incumbent histories, worst temporary score, area/cell/turn tradeoffs, route expansions, probes,
elapsed time and memory. Compare actual delivered incumbents, not training reward alone.
Audit whether a discovered path crosses a strictly worse score and whether a bounded exhaustive
or sufficiently explicit local control establishes that monotone paths cannot achieve the same
result in that precisely defined neighborhood. Do not call a merely observed detour necessary.

Measure independent environment worker counts and PyTorch compute thread counts separately.
Use available CPU capacity with bounded processes and avoid worker-times-thread oversubscription.
A persistent validator protocol may remove repeated catalog parsing/process startup, but must
execute the identical independent Rust checks. Compare stream and file results and preserve the
existing file CLI. Do not alter validation semantics for throughput.

## Outputs, failures and completion

CLI-only schema-1 JSON plans, provenance, immutable trajectory/checkpoint/audit artifacts,
machine-readable work/quality summaries, and a findings report with a calibrated verdict.
Every diagnostic has stable code, severity, stage, message and relevant entity/path. Missing
or stale input, output reuse, process timeout, resource exhaustion and unsupported geometry
are explicit; none means global infeasibility. JSON-only model exchange, Python via uv,
CPU only unless the user separately authorizes an accelerator environment.

Completion requires tested state/return/serialization/budget semantics, actual generator-root
imports from both generations, multi-seed long-horizon versus myopic/nonlearning experiments,
CPU scaling measurements, fresh final audits, workspace verification and focused commits.
Evaluate against the accepted constructive-planner design before each commit. The verdict may
be that the action space or current learner is inadequate; identify which evidence supports it.
