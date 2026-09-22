# Hierarchical search performance and long-run readiness

The integrated empty-session factory case remained independently valid while
the lower and outer inference graphs were compiled and the exact CPU search
pool was scaled. All measurements used the generation-1 checkpoints, an NVIDIA
GeForce RTX 5090, Torch `2.13.0+cu130`, and the full-factory panel. The primary
metric is complete macro candidates sampled per minute.

## Compiled inference

Three release runs were made after a complete unmeasured warmup of the same
integrated search. Both modes completed and independently validated the case in
every run.

| Mode | Beam median samples/min | MCTS median samples/min |
| --- | ---: | ---: |
| Eager | 1,770,402 | 387,639 |
| Compiled | **1,840,888** | **417,249** |
| Change | +3.98% | +7.64% |

The compiled path freezes detached lower and outer parameters, traces the real
tensor-only inference graphs, checks the first result against eager inference,
and then reuses the trace. Lower traces are cached by graph count and raster
dimensions; the outer trace accepts varying action counts. Training continues
to use the eager models. The retained LibTorch profiling executor incorrectly
specialized an empty spatial-token tensor on the second invocation, so frozen
inference uses the non-profiling TorchScript executor. A separate four-case
route panel exercised the other batch signatures and completed 4/4 with both
beam and MCTS.

Matched Nsight Systems runs measured 298,039 eager and 319,378 compiled MCTS
samples/min (+7.16%). Compilation reduced CUDA runtime calls from 368,494 to
317,326 (-13.9%), runtime-call time from 948.1 ms to 878.9 ms (-7.3%), kernels
from 153,185 to 129,264 (-15.6%), and kernel time from 187.6 ms to 168.7 ms.
Kernel launches fell from 148,386 calls and 367.8 ms to 124,435 calls and 316.1
ms (-16.1% calls and -14.1% launch time). This supports retaining compilation:
the gain comes from less dispatch work rather than a changed search budget.

## CPU pool sizing

Each point is the median of three release runs with compiled inference. The
candidate budget, models, GPU and panel were fixed.

| CPU threads | Beam samples/min | MCTS samples/min |
| ---: | ---: | ---: |
| 1 | 1,019,442 | 271,525 |
| 2 | 1,464,061 | 357,245 |
| 4 | 1,814,463 | 411,562 |
| 8 | **1,834,528** | **417,845** |
| 16 | 1,839,089 | 417,286 |
| 24 | 1,828,523 | 418,670 |

Eight threads are the smallest saturation point and remain the checked-in
default. MCTS improves 53.9% from one to eight threads; adding threads beyond
eight changes throughput by about 0.2% and does not justify the additional CPU
occupancy. CPU samples showed exact routing hash/ordered-map/A* work spread
across the pool while the main thread issued GPU work, consistent with the
measured scaling.

## Interrupted generation recovery

Generation distillation now checkpoints only at configured epoch boundaries.
Each checkpoint binds the exact config, panel, collection, catalogs and source
checkpoint hashes, and atomically publishes both model parameters and all Adam
step, mean and square tensors. A latest pointer is replaced only after both
files and their hashes are complete; older recovery checkpoints are removed.
Final generation publication remains an atomic directory rename.
The generation report schema advances to v2 to expose whether the publishing
invocation resumed a checkpoint; existing v1 configs remain valid and receive
a bounded 50-epoch checkpoint default when the new field is absent.

A bounded CUDA validation terminated a 200-epoch generation after epoch 60,
then resumed it in a new process. It completed all 600 optimizer updates and
28,800 samples at 442,825 samples/min. Its final outer loss exactly matched the
uninterrupted control to the reported precision; the lower loss differed by
`5.3e-6`. The resumed checkpoints then completed and independently validated
the full-factory case with beam and MCTS. This validates process recovery,
optimizer restoration and the published model workflow without launching a
long training run.

The final readiness run used the separate 18-case `training` panel for frozen
generation-1 collection. Beam and MCTS completed 18/18 cases at 119,007 and
100,095 samples/min. Distillation processed 36 root states, 324 outer actions
and 324 lower routes for 200 epochs: 129,600 samples and 2,400 updates at
701,661 samples/min. Outer loss moved from 1.8502 to 1.7568 and lower loss from
2.1438 to 1.7087. The generation-2 models then completed 4/4 fixed held-out
cases with both searches and completed the independent empty-session
full-factory case with both searches. Held-out MCTS measured 116,697
samples/min and full-factory MCTS measured 472,281 samples/min. This is the
promotion evidence for the prepared generation; it does not reuse training
case identities for evaluation.

A long run repeats that boundary: collect with immutable generation `g` on the
training panel, distill into pending generation `g+1`, resume that pending
directory with `--resume` after interruption, evaluate the published `g+1` on
the fixed held-out and full-factory panels, and promote only after completion
and throughput checks pass. The CLI rejects a changed config, collection,
catalog, panel or source model during resume.

The source implementation for compiled inference is `a402b50`. Raw Nsight
reports, the 18 CPU-scaling reports, interrupted recovery output, the separated
generation-2 collection/checkpoints and the final validation reports are
retained under the isolated server checkout's `artifacts/` directory.
