# CPU learning feasibility verdict

Date: 2026-09-07. Status: measured, independently audited, isolated experiment.
Machine: Apple M4, 24 GiB RAM, macOS 26.6, Python 3.14.7, PyTorch 2.14.0.
All policies used CPU tensors and one PyTorch compute thread; neither MPS nor CUDA was used.

## Verdict

**GO for CPU-based learning research and a small learned local-search component.** Real
held-out signal response, negative controls and a fixed-test data-scale response were measured.
CPU arithmetic is not the immediate obstacle at the tested scales.

**NO-GO for replacing the current geometric planner with this model.** A screened geometric
baseline substantially outperforms it. The learned policy is a working prototype, not a
demonstrated practical improvement over a competent nonlearned method.

**Conditional GO for pursuing user-level or better quality, not a claim it is already feasible
at an established training budget.** A learned proposal/repair policy combined with exact
rule screening, routing and transactional validation is a credible development path. The
experiments do not establish that CPU-only training will reach that quality, how much data
it will require, or that simply scaling this architecture will work. The next decisive gate
is a same-request, fully validated human-reference benchmark and a permitted geometry corpus,
not a larger GPU purchase or more repetitions of this tiny-chain distribution.

The verdict answers feasibility at two levels: computation is demonstrated affordable here;
human-quality generalization remains a research risk. It would be misleading to collapse those
into either an unconditional promise or an assertion of impossibility.

## What was trained

The policy proposes one concrete upstream-facility relocation, rotation and port choice from
twelve shuffled alternatives. A* routes the changed edge; the independent Rust probe and exact
recipe-margin check accept or reject the fragment. Existing valid geometry is retained unless
the proposal validates and strictly improves area, then transport cells, then turns.

There are only two- and three-facility chains, with 3–45 current-state geometry tokens. Their
external obligations remain pending. None is a complete factory. Main-family tests hold out
geometry, not unseen graph topologies; the separate pipe family tests a limited distribution
shift. Raw numeric flow, instance IDs, action ordinal and post-action labels are absent from
the model input. Capacity requirements are preprocessed to one supported lane per edge.

The primary model has 15,969 parameters: two directed graph-message blocks, width 32,
two-head action-to-state attention and a shared scoring head. The pooled MLP control has
3,297 parameters. See [research.md](research.md) for ten primary-source papers and the explicit
distinction between borrowing architectural ideas and reproducing published results.

This is sampled-action REINFORCE on a **cached one-step contextual bandit**, not online,
long-horizon reinforcement learning. Counterfactual labels were expensive environment outputs
generated in advance, not free samples. A sampled update reads its chosen reward; the separate
supervised control consumes all best-action labels. Each run used 600 updates of batch 64,
Adam at 0.001, with seeds 7, 17 and 29. No best-seed/checkpoint selection was performed.

## Learning signal and negative controls

Initial dataset: 192 training, 48 validation, 64 same-family test and 48 pipe-family states.
Reward is an ordinal ranking within each finite candidate pool: invalid/unresolved proposals
receive 0, accepted proposals receive 0.5–1, and 1 means best in that pool. It is not a percent
of globally optimal layout quality. The table reports mean argmax-policy metrics over all
three seeds; uniform choice is evaluated analytically.

| Method | Test reward | Best-candidate hit | Proposal validity |
| --- | ---: | ---: | ---: |
| Untrained / zero reward | 0.564 | 5.2% | 79.7% |
| Shuffled rewards | 0.597 | 7.3% | 82.3% |
| Uniform random | 0.618 | 8.3% | 82.4% |
| Graph policy, normal rewards | 0.688 | 18.2% | 83.3% |
| Pooled MLP, normal rewards | 0.709 | 21.4% | 82.8% |
| Screened geometry, no learning | 0.953 | 75.0% | 98.4% |
| Full cached-candidate oracle | 1.000 | 100% | 100% |

Every normal-reward graph seed beats its matched shuffled and zero controls. All zero-reward
models preserve every weight exactly. This supports a genuine signal response. It does not
support a claim that graph attention is necessary: the smaller MLP is better on this first
test. The unchecked bounding-area heuristic is deliberately retained in the artifacts but
is not the relevant strong baseline: its frequent obvious collisions make it too weak.

The supervised graph control obtains training reward 0.993–1.000 but only 0.577–0.689 on test.
That is strong evidence of overfitting/input-generalization difficulty in this regime, not
proof that a larger network is needed. Its best-action loss also differs from expected-reward
training, so best-hit rate and proposal validity can move in different directions.

## Does more data help?

A second seed generated 2,112 states: 1,536 train, 192 validation, 256 test and 128 pipe-family
test. Both training sizes use the **same held-out states** and the same 600 updates. The small
condition uses only the first 192 training states; normalization also uses only those states.
The large condition does not get more updates or a selected checkpoint.

| Method | Train states | Test reward, mean [seed min, max] | Test proposal validity |
| --- | ---: | ---: | ---: |
| Graph, normal reward | 192 | 0.684 [0.661, 0.705] | 83.6% |
| Graph, normal reward | 1,536 | 0.760 [0.751, 0.767] | 92.6% |
| Pooled MLP, normal reward | 192 | 0.681 [0.669, 0.686] | 83.6% |
| Pooled MLP, normal reward | 1,536 | 0.738 [0.723, 0.754] | 86.5% |
| Screened geometry | None | 0.971 | 100% |

All three matched graph seeds improve with more data; the train/test reward gap shrinks.
However, best-candidate hit remains only 19.3% for the large graph policy versus 75% for the
screened baseline. Pipe-family reward improves only 0.727 → 0.743; its uniform baseline is
0.720 and screened geometry is 0.987. This is not convincing broad transfer.

These are descriptive results over three seeds and synthetic held-out states, not confidence
intervals, a scaling law, or evidence that eightfold data growth will keep producing the same
gain. All seeds, curves, checkpoint hashes and counterfactual-label access counts are recorded
in [results.json](../../../artifacts/benchmarks/layout-learning/results.json); full raw reports and models remain under `target/`.

## CPU cost and the actual bottleneck

| Work | Measured cost |
| --- | ---: |
| Initial 352-state environment generation | 13.75 s; 4,559 candidate evaluations; 4,138 probes |
| Larger 2,112-state environment generation | 92.46 s; 28,240 candidate evaluations; 25,730 probes |
| Graph training, 600 cached updates | About 2.0–2.1 s per seed/run |
| Pooled MLP training, same update count | About 0.3 s per seed/run |
| Whole initial 15-run training CLI | 27.62 s, including evaluation and JSON checkpoints |
| Whole 9-run data-scale CLIs | 14.99 s / 17.07 s |
| Warm graph forward, one existing test state | About 0.10 ms median; excludes feature creation and routing |

Each bandit run makes 38,400 sampled reward observations, repeatedly using a finite table.
For example, the initial graph policies observe only 2,196–2,252 unique state/action pairs
out of 2,304 training pairs. Cached samples/second must not be reported as new online validated
layouts/second. The full precomputation, including rejected task attempts, is charged.

For the large dataset, independent process execution accounts for 80.01 s, evidence writes
for 6.56 s, and A* itself for 0.96 s. A persistent/batched independent checker is therefore a
more concrete future throughput opportunity than a GPU for this small workload. This change
was not implemented. Filesystem allocation is roughly 88 MiB for the initial data and 546 MiB
for the large data; per-probe immutable evidence is intentionally verbose.

A separate random-tensor cost test extends beyond actual learned states. At 1,024 tokens,
width 128, 235,905 parameters and batch 16, median single inference is 1.64 ms and an Adam
update is 90.84 ms. Process cumulative peak RSS is about 559 MiB, including PyTorch. These
numbers show arithmetic headroom, **not successful training on 1,024-token factories**. The
dense adjacency grows quadratically; larger regions need a measured sparse/hierarchical
representation, not silent geometry truncation. Hardware timings are development-machine
observations, not an isolated-host benchmark or extrapolated RTX 5090 speedup.

## Community data and the path to human quality

[data-sources.md](data-sources.md) records the source audit. Real machine-readable community
geometry exists: eight Chinese IndustrialPlanner examples and two simulated layouts among
twelve popular EndfieldTools samples. Talos Pioneers reports 1,669 entries, but its inspected
records expose metadata/share codes/images, not entity coordinates. None of those counts is
a verified, diverse, licensed training-corpus size. No community content trained these models.

A defensible next milestone would:

1. Obtain permitted geometry exports and map them to the exact external catalogs. Independently
   check ports, recipe inventory/utilization, transport rules, external boundaries and version
   semantics; do not trust a simulator's `isValid` field or ignore-stock settings.
2. Split by original creator/design, production topology and catalog version before generating
   perturbations. Rotations and edits of eight designs are not thousands of independent designs.
3. Use cheap exact geometry masks and a nonlearned teacher as baselines. Learn bounded connected
   placement/repair decisions where there is measured headroom, preserving immediate routing
   and commit-only-on-improvement behavior. Geometry imitation alone cannot exceed a teacher;
   verified search-generated improvements can supply better targets.
4. Evaluate full factories on the same requests, rates, boundaries, time budgets and score
   ordering as the human references, including failures and downstream closure. Add the user's
   visual preferences as explicit evaluation criteria without silently replacing score priority.

The existing screenshot references contain 8, 4 and 9 facilities and pending external margins
([reference report](../chain-prototype/reference-report.md)). This policy was not trained or
tested against them. Comparing its two-/three-facility areas with theirs would be invalid.
The recent [GOAL](https://proceedings.iclr.cc/paper_files/paper/2025/file/826aea2253363fe04e8c4991b2a8869e-Paper-Conference.pdf),
[CaDA](https://proceedings.mlr.press/v267/li25bi.html) and
[MaskCO](https://proceedings.iclr.cc/paper_files/paper/2026/file/d85816bf41e54d9b0847e02249f4fcc6-Paper-Conference.pdf)
papers motivate structured context and constrained repair, but their results do not establish
Endfield performance or a CPU training budget.

## Integrity, verification and remaining risk

- All splits exclude translation/legal-global-rotation-equivalent current states. The large
  generator records 50 uninformative and 101 duplicate attempts; accepted-task conditioning
  is not an unbiased environment success rate. Route-budget outcomes remain local unknowns.
- A smoke rerun reproduces all 36 state fingerprints, features and 432 candidate outcomes
  exactly, excluding timing/provenance bytes. The first development smoke exposed a missing
  whole-layout coordinate translation for the Rust DTO; it was fixed and regression-tested
  before training. The original faulty smoke is not used as learning evidence.
- Fresh audits replay 943, 2,204 and 2,020 unique selected actions for the three reports,
  including retained, geometric and oracle controls. Validation, scores and canonical witness
  hashes match. Those sets overlap; their sum is not a count of unique independent examples.
- Reloaded JSON checkpoints reproduce all 1,680 + 3,456 + 3,456 reported held-out selections.
  Rejected/non-improving proposals retain the current layout; none creates factory validity.
- Six Python tests and 358 Node regression tests pass. `cargo check --workspace` and
  `cargo test --workspace` pass (524 tests; two pre-existing ignored tests). Missing input,
  output-directory reuse and unavailable CUDA were checked manually and fail explicitly.
- The model inputs/weights contain no raw rates or hidden candidate outcomes. Source hashes
  prevent silently training/auditing with changed catalogs or environment code. The test suite
  checks padding, node/action permutation, gradients, zero-update behavior and JSON round trips.
- Production commands, Cargo contracts, exact research solvers and game catalogs are unchanged.
  The accepted constructive design was checked: routed connected prefixes and transactional
  local improvements are preserved; no full-factory success or global infeasibility is claimed.
- This is not a production trainer: no long-run checkpoint recovery, distributed rollout,
  generalized topology curriculum or reward-policy deployment is included. NumPy is deliberately
  not a dependency; PyTorch emits an optional NumPy initialization warning, but no NumPy path
  is used by these tested commands.
