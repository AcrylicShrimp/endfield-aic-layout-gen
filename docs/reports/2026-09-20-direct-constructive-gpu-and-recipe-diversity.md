# Direct constructive GPU and recipe-diversity experiment

## Result

The constructive learner now uses the training server GPU for workloads large
enough to benefit, prepares disjoint logical goals from the normalized game
recipes, and bounds PPO activation memory with gradient-accumulated
minibatches. These changes address two limitations in the first pilot: its four
nearly identical authored goals and its repeated transfer of packed action
logits from the GPU to the CPU.

Across initialization seeds 101, 202 and 303, the final memory-bounded recipe
experiment improved held-out stochastic completion from **54/1,536 (3.52%)**
before training to **354/1,536 (23.05%)** after 32 updates. Every seed improved.
Greedy completion was transient and ended at zero in all three arms, so the
broader distribution establishes a robust stochastic learning signal but does
not yet establish stable modal decoding.

## CUDA execution boundary

The CLI device default is `auto`: it selects `cuda:0` when CUDA is available and
otherwise uses CPU. The training report records the resolved device and strict
determinism mode.

Conditional sampling previously copied every packed factor distribution to CPU.
The CUDA path now draws one CPU generator value per active graph, transfers only
that compact draw vector, constructs a packed CDF on the device, and returns only
the selected indices and log probabilities. The CDF accumulates in float64; a
float32 global CDF lost segment resolution at 1,024 graphs and produced an
out-of-range selection during the first server scale test.

Release-mode integrated timings on the RTX 5090 show why device choice depends
on workload size:

| Model and episode batch | CPU | CUDA | CUDA speedup |
| --- | ---: | ---: | ---: |
| width 32, 64 episodes | 0.82 s | 1.96 s | 0.42× |
| width 32, 1,024 episodes | 4.27 s | 1.93 s | 2.21× |
| width 128, 256 episodes | 4.96 s | 1.79 s | 2.77× |
| width 128, 1,024 episodes | 20.61 s | 2.42 s | 8.50× |

The smallest pilot remains faster on CPU because its serial native turns and
small kernels cannot amortize CUDA launch and transfer cost. The recipe-diverse
run uses width 128 and 256 episodes, where CUDA has a clear measured advantage.

## Recipe-derived logical goals

`aic learning prepare-constructive-problems` loads and validates normalized
recipes, items and physical catalogs on every invocation. It constructs every
producer-to-consumer recipe edge for an internally produced input, then uses a
separate seeded generator to select at most one edge per target recipe. Training
and evaluation therefore have disjoint target-recipe identities rather than
merely different block IDs.

Seed 101 found 2,192 dependency edges across 296 eligible target recipes. The
complete target-balanced split contains 232 training and 64 evaluation problems:

| Coverage | Selected count |
| --- | ---: |
| Target recipes | 296 |
| Facility pairs | 76 |
| Commodities | 148 |
| Belt problems | 216 |
| Pipe problems | 80 |
| Training/evaluation target overlap | 0 |

A problem contains two real recipe facilities, the real intermediate item and
its transport kind. Its flow is the smaller of producer output and consumer
input rate. Three selected flows exceed one transport line, so the pairwise
problem limits those three to the catalog line capacity; multi-line production
balance remains outside this pairwise experiment.

Recipe and commodity IDs are provenance, not learned categorical embeddings.
The policy initially observes facility footprints, transport and rate; port
choice counts add geometry after placement. Under those model-visible features,
the split contains 32 distinct training tuples and 18 evaluation tuples (17 and
11 if rate is omitted). This is a substantial expansion from the two effective
rate/geometry patterns in the authored pilot, but it remains a bounded pairwise
distribution rather than full-factory diversity. The prepared pool is fixed by
its seed and cycled during training; trajectory commands remain stochastic.

The generated problem file is 202,626 bytes and is intentionally regenerable
rather than tracked. Its SHA-256 is
`c1b926d0f08c179de58d9a2c340120b858ac16ea2929ac5033252b7aaa2e40ba`.
The recipe and item source hashes are
`0fa9f71b6ab2c66ef774ad54f27ddac8682c4ccd6134730d69ca7506836f1ad2`
and `b49b70d60bee6dc421817e0f6a793532866dfe8618c2e9111c17d032e3fc9164`.

## Three-seed training result before memory correction

The first larger experiment used a 12 by 12 placement canvas, extension lengths 1–4,
a 40-turn horizon, width 128, four graph rounds, 256 training episodes per
update, four PPO epochs, 512 held-out stochastic episodes and 32 updates. Each
run processes 8,192 training episodes. The three runs took 54–56 seconds each
on the RTX 5090.

| Seed | Stochastic initial | Stochastic final | First 8 mean | Last 8 mean | Greedy final | Training successes |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 101 | 21/512 | 169/512 | 32.25 | 134.75 | 47/64 | 1,230/8,192 |
| 202 | 18/512 | 118/512 | 28.63 | 109.38 | 0/64 | 1,116/8,192 |
| 303 | 15/512 | 132/512 | 32.00 | 117.00 | 32/64 | 1,147/8,192 |

The metrics SHA-256 values are:

- seed 101: `b5c2633bbf26e73925d23f032bc8fb3f2995cff7ae7298dce2a7f1a9d05ef0e2`
- seed 202: `6cf33347b1793b643fea750305e045e6354a14fb0a509b233dc7d44aa19191a0`
- seed 303: `e1c79bcb85042fd7b7f563adda9de9432f88a91abd9576131f92645b07f6d693`

These runs omitted a PPO minibatch bound. Their aggregate final greedy result
was 79/192, but seed 202 ended at zero after reaching 35/64 and the successful
seeds changed abruptly between evaluations. This is evidence of modal
instability rather than a stable 41% greedy policy.

## VRAM diagnosis and correction

The first scaled PPO implementation built one autograd graph for every
transition and all of its conditional factor choices in the complete rollout.
A 1,024-episode recipe batch exhausted the 32,607 MiB device and was stopped.
Even the successful 256-episode runs reached the allocator limit and emitted
retry warnings. The model has only 520,834 parameters; retained choice
activations, not parameters, caused the memory use.

`ppo_minibatch_size` now bounds the number of transitions packed into one
backward graph. The learner normalizes advantages over the complete rollout,
zeros gradients once per PPO epoch, weights each minibatch loss by its share of
the complete rollout, accumulates gradients, and performs one clipped Adam step
after all minibatches. This preserves the full-batch update count and learning
rate instead of turning every memory slice into another optimizer step.

With 9,736 transitions split into 20 minibatches of at most 512, the one-update
integrated run used **1,250 MiB peak VRAM** and finished four PPO epochs in 3.10
seconds. Its first-epoch loss and actor/critic gradient norms matched the prior
full-batch values within floating-point summation error.

The complete 32-update memory-bounded replication stayed at or below **2,004
MiB peak VRAM** and took 50.7–53.2 seconds per seed, slightly less than the
54–56 second unbounded runs:

| Seed | Stochastic initial | Stochastic final | First 8 mean | Last 8 mean | Greedy peak/final | Peak VRAM |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 101 | 21/512 | 104/512 | 32.25 | 84.63 | 37/0 | 1,568 MiB |
| 202 | 18/512 | 116/512 | 28.63 | 106.38 | 4/0 | 1,786 MiB |
| 303 | 15/512 | 134/512 | 32.00 | 120.00 | 0/0 | 2,004 MiB |

The memory-bounded metrics SHA-256 values are, respectively,
`6a98b184a6031cf7fd6d25dd4c2bca4faf0e78e8a7e079c3eb7f549e1ac2a3bb`,
`a47e447b3572b74738cb6b12f9dbd296588a0753f1f0063b729b95a98f735e54`
and `45c0e5da4eff04fe3022dd130135c11c77c94a4b0c310e99047f832e00b9cf1c`.
Seeds 202 and 303 ended within two successes of their full-batch arms; seed 101
diverged more strongly. Tiny summation differences alter later autoregressive
branches, so exact trajectory replay is neither expected nor required. The
three-seed comparison shows no systematic loss of the stochastic learning
signal, while greedy decoding remains too unstable for a quality claim.

After retaining the final metrics, checkpoints and prepared input, the
post-training cache cleanup recovered 13,702,201,344 bytes. Server free space
increased from 31,001,071,616 to 44,703,272,960 bytes. The byte-precise record
is `~/Srv/aic-direct-constructive-20260920/artifacts/server-cleanup-20260920/post-training-cache-cleanup.json`.
The final build, focused tests and schema-cutover CLI check recreated a debug
cache and one disposable output. Removing both recovered another 3,625,295,872
bytes and left 44,703,252,480 bytes available; the record is
`~/Srv/aic-direct-constructive-20260920/artifacts/server-cleanup-20260920/final-verification-cache-cleanup.json`.

## Reproduction

The retained training input is
[`config.json`](../benchmarks/direct-constructive-recipe-diverse/config.json).
It now uses training schema v2, which makes its existing 512-transition PPO
minibatch declaration mandatory; the measured v1 run had the same value and
training behavior. No v1 compatibility branch remains.
Generate the logical split and train it on the documented server as follows:

```bash
aic learning prepare-constructive-problems \
  --config docs/benchmarks/direct-constructive-recipe-diverse/config.json \
  --catalog-root data/game/normalized \
  --output target/recipe-pairwise-problems-101.json \
  --seed 101 --training 232 --evaluation 64

aic learning train-constructive \
  --config docs/benchmarks/direct-constructive-recipe-diverse/config.json \
  --catalog-root data/game/normalized \
  --problems target/recipe-pairwise-problems-101.json \
  --output target/recipe-diverse-training-101 \
  --device auto
```
