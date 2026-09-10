# Actor/critic combination readiness

The registered shared/separate backbone, critic history and critic schedule
treatments are implemented. Four native readiness cells cover both backbone and
history choices with critic tails enabled. Each completed two training batches
and 992 actions (31 accepted factories across each 32-attempt two-batch schedule).
The deliberately small KL threshold 1e-8 forces the branch under test; it is not
the threshold used for the learning comparison, which remains 0.02.

The independent audit of each readiness cell verified 48 result records,
native initial/best factory validity, recorded rewards and observation hashes,
CPU collection predictions against padded CUDA predictions, and exact replay of
batch two from a nonempty Adam checkpoint. All four replays reproduced policy,
critic, both optimizer and shuffle hashes and every joint epoch diagnostic.
Each executed one actor epoch and four critic epochs. The critic-only tail
preserved actor parameters, actor Adam and the joint shuffle stream exactly.

Shared current-world and recurrent features are computed once for both heads.
Best-layout inputs remain hidden from the actor. Shared parameters have exactly
one optimizer owner and are clipped once. During the shared-model critic tail,
only the private value head learns; independent models can update their entire
critic. This is a documented interaction in the factorial experiment.

Prediction diagnostics compare pre-action values with realized remaining rewards
in both training and evaluation. They include failure penalties and separate
initial, early and late positions. Evaluation observations never enter an update.
All cells allocate the same history-capable value head and begin with identical
per-seed actor/critic tensors; history-off cells mask those features to zero.

## Verification

Focused tests cover shared scalar/padded forward and gradient equivalence,
hidden-best isolation, history feature effects, optimizer ownership, forced KL
tails, exact resumed updates and weighted predictive-error arithmetic. Local PPO
tests passed (87 tests, three platform-specific skips). On Linux, all 85 runtime
PPO tests passed without skips with the native library and deterministic IEEE
FP32 CUDA settings. The six combination tests exercised CUDA. Two additional
local tests verify matrix controls and factorial main/interaction effects.
Workspace check and tests passed; no Rust implementation changed.

An initial CUDA test invocation omitted the harness's deterministic settings and
failed exact hash comparisons. Repeating with those settings passed without
changing the learner or loosening tolerances. The test setup now explicitly uses
the same settings. A GRU contiguity warning occurred in a deep-copied test model;
it did not invalidate forward/gradient or checkpoint comparisons.

## Retained evidence

Readiness root:
`/home/ashrimp/Srv/aic-ppo-combinations-20260911/artifacts/factory-edit-pilot/readiness`

Each cell is stored under its name; its audit is in `<name>-audit/report.json`.

| Cell | Protocol SHA-256 | Audit SHA-256 |
| --- | --- | --- |
| shared-history-tail | ce22c224a9f33b37000cbb53d71faf50ccef4dcc7935ee4189feb9c43c8f5255 | c55a30cb1f584edbf3e1895305f84205ba60ab2fc8d754cdf71d8d7b5492025a |
| separate-history-tail | fdbc7756ab353ca54f24594c38abd2f9b4addb719ed8f6b85bfe2c9041d54944 | 60c750d7a7c6256034c6e463662cd1a0b6c376f96768a8748f1aef247a31f364 |
| separate-nohistory-tail | e29b96933fc00d6adc779b2196a38ab39c4550ebaa0367ad88a03b748f688738 | 1f0aa4ce22650d0c1e0dd4fe55a8aa0afdc2a622604a772cfeb9c36ae101bd81 |
| shared-nohistory-tail | 7a478b2380f4899401ed2ebd6b71a2bb1efcf91fa59fc8e72d613de41ee4393b | 90df9407bc9909cbdb1d2615465dd2f501a9a61497b6914f32ea6d19067d0706 |

The [design](../designs/2026-09-11-ppo-actor-critic-combinations.md) and checked-in
screen configuration define the subsequent eight-cell, two-seed comparison.
Readiness results establish implementation behavior, not comparative quality.
