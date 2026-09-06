# CPU learning signal experiment

Status: completed and audited; contract established before implementation (2026-09-07).

The user subsequently made this a goal: deliver an experimental, calibrated CPU feasibility
verdict and assess a credible path toward matching/exceeding their layouts. This authorizes
additional bounded data-scale/cost experiments and public blueprint-source research, not a
claim that the small-chain policy already handles full factories.

## Question and scope

Does a small PyTorch policy respond to a mechanically generated layout-quality signal on
held-out synthetic states, and what do environment generation, CPU training and CPU inference
cost separately? Strong factory quality, superiority over geometric heuristics and full
Heavy Xiranite support are not acceptance requirements for this first experiment.

This is an isolated Python/uv learning experiment with a Node geometry environment. No
production entry point, previous constructor, unrelated work or game catalog is changed.
CPU is explicit by default; CUDA may be selected explicitly later, never silently substituted.
The user offered an RTX 5090 but no remote machine access or GPU run is assumed.

## Environment and actions

Input: explicit catalog paths, independent release validator, schema-1 chain graph request
paths and a schema-1 experiment plan. Reuse exact externally loaded recipe/utilization/flow
ledgers. This initial environment supports two- and three-facility simple chains with one
capacity lane per edge. It is not a mixed-kind full factory or general graph optimizer.

A state is an independently checked, internally routed chain with its original external
margins still pending. The final upstream facility is movable. Earlier facilities and their
internal routes are frozen. For three facilities, the two-facility prefix is placed, routed
and checked before adding the final facility. A move chooses a new position, catalog-allowed
rotation, output port and target input port, then immediately reroutes the affected edge.
Candidate generation samples relative positions/ports, not one of four named heuristic modes.
Its finite proposal neighborhood is diagnostic, not a game canvas or global feasibility limit.

Each state includes a retained current action and a fixed-size shuffled candidate set. Every
candidate is routed/checked under the same per-action limits. Failed candidates remain visible;
timeouts are local unknown outcomes, not global infeasibility. Only validated score improvements
may replace the retained chain. An external-margin fragment is never a complete factory.

## Synthetic data and leakage controls

Generate varied sink orientations, frozen connected prefixes and movable-facility poses from
the supplied graph/catalogs. Require at least two accepted candidates with distinct scores to
make each selected training task informative. Record all generation attempts, rejected tasks
and work; this conditioning is a limitation, not an unqualified environment success rate.

Split by a canonical current-state geometry fingerprint, including legal global rotations and
translation equivalence, not by individual candidate rows. Never allow a transformed copy of
the same state across train/validation/test. Reserve a separate request family for an optional
out-of-family test; same-family geometric holdout is not unseen-topology generalization.

Model input contains pre-routing relative geometry, current facility/transport tokens,
directed connectivity and selected-port positions/directions. Exact numeric flow, identifiers,
candidate ordinal, validator output, routed candidate score and reward are excluded. Connection
capacity requirements are fixed by preprocessing. Feature normalization uses training data only.
The explicit input field allowlist and tests must establish this separation.

The dataset stores the complete counterfactual outcome table for repeatable evaluation. Its
generation cost is charged and reported in full. This is not free online simulation. An RL
update observes only the reward for its sampled action; a supervised control may use all labels
and must be identified separately.

## Learning signal and controls

Score each accepted internal fragment lexicographically by measured used area, physical
transport cells, then port-arm-inclusive turns. Convert the distinct scores in one fixed
candidate set to an ordinal reward, with all accepted candidates above failed candidates and
ties equal. This reward is local to that finite candidate set; it is neither a global bound
nor a calibrated utility across arbitrary factories.

Following the user's architecture/recent-research request, the primary model is a small directed
graph encoder with action-to-state cross-attention; a pooled MLP is a control. See
[research and architecture](research.md). Train a categorical policy with sampled-action REINFORCE
(one-step contextual bandit, not long-horizon RL). Use a training-only baseline and fixed
hyperparameters/seeds. Test random-label and constant-zero-reward controls with the same
initialization and work. A supervised best-candidate control distinguishes optimization
difficulty from network/input capacity. Include uniform random, untrained policy, retain-current
and a cheap pre-route geometry heuristic. Do not claim learned superiority unless measured.

After the first small run, add a stronger geometry control that excludes obvious body/endpoint
collisions before ranking bounding area and Manhattan distance. This prevents an unnecessarily
weak baseline from overstating learned value. For the enlarged goal, freeze a second dataset
seed and compare 192 versus 1,536 training states on exactly the same held-out states, with
the same 600 updates, architecture, learning rate and all three seeds. No best-test checkpoint
selection or hyperparameter search. This is a data-scale diagnostic, not independent topology
generalization. Separately measure larger tensor workloads as cost-only stress probes; random
tensors do not count as validated layouts or successful training.

Report training/validation curves, held-out proposal validity, best-candidate hit rate,
normalized reward/regret, safe-commit improvement frequency, per-request applied geometry,
sampled reward observations and unique observed state/action pairs. Do not select the best
seed or tune on test outcomes. Complete artifacts expose all seeds and negative controls.

## CLI artifacts and diagnostics

Generation emits immutable plan/source/catalog/code/probe hashes, exact states/actions,
features, outcomes, witnesses and independent validation reports, split fingerprints and a
separate work/timing report. Training emits versioned JSON model weights and normalization,
learning curves, selected actions, seed/control results, runtime versions, device/threads,
parameter count and measured training/inference timing. Avoid pickle for exchange artifacts.
Post-training audit freshly reroutes and independently validates selected held-out actions
against original graphs; record it separately from cached training throughput.

All commands use explicit flags and fresh output directories. Diagnostics contain stable code,
severity, stage, message and relevant entity/path. Missing dependencies, invalid input, missing
CUDA, stale provenance, output reuse, nonfinite gradients or verification failure are explicit.
Report timeout/work failures honestly; do not repair labels or substitute another engine.

## Completion gates

1. Reproducible synthesized data with real routing and independent checks; no split leakage.
2. Tested PyTorch gradients/update, serialization, normalization, signal controls and CLI failures.
3. Fixed-seed CPU experiments covering normal, shuffled and zero signals, plus supervised and
   nonlearned baselines; report whether learning occurs, even if performance is poor.
4. Separate environment, training and inference measurements; no inferred GPU speedup.
5. Fresh selected-action audits, broad existing regression/workspace verification, a findings
   report and focused commit. No production integration or further learning milestone is implied.
