# Search-coupled hierarchical generation

This run verifies one complete frozen-model generation: collect exact MCTS root
records with generation 0, distill visits and Q values into both models, then
evaluate the published generation 1 checkpoints without changing the panel or
search budget.

- Source: `1657fe3f550a8a7aa1f164ab55451ada0e3294cf`
- Device: NVIDIA GeForce RTX 5090, Torch `2.13.0+cu130`
- Generation 0 lower checkpoint:
  `d88f5c1e7b9c49b0bc9f57acdf1cd7e63aaf0f1d93e4330ff3b28a51c0e80b1b`
- Generation 0 outer checkpoint:
  `32fb8174f159b293ce21d6e0592f35808a953ceb55ab91ae82f599fd9639df75`
- Generation 1 lower checkpoint:
  `0b51d70ee03d05b1aaf7793521807127fde784aac53d6d863c5c1ca285ce19ef`
- Generation 1 outer checkpoint:
  `c1f641905f1fcd3a3611bb1294f10311ad1e28928a5d7fa29dd0f13726213f54`

The collection retained eight exact root states, 72 exposed macro actions, 504
MCTS edge visits, all selected actions and all eight later successful native
outcomes. Replay verified every state hash and action witness before training.
The 200-epoch distillation processed 28,800 samples in 3.369 seconds, or
**512,842 samples/min**. Outer loss changed from 1.682 to 1.441; lower loss
changed from 18.788 to 1.856.

| Frozen evaluation | Beam valid | Beam samples/min | MCTS valid | MCTS samples/min |
| --- | ---: | ---: | ---: | ---: |
| Generation 0 | 4/4 | 81,965 | 4/4 | 98,376 |
| Generation 1 | 4/4 | 95,877 | 4/4 | 98,791 |

The generation update preserved all completions and MCTS throughput. This small
panel proves lineage, replay and target flow; it does not establish improvement
on broader layouts. The retained raw SHA-256 values are
`4fb5c8ce67bde705d42d283cc8699850ecc81c6c238896d70e7ea76bbb171ea4`
for generation 0 collection,
`b23a488dd22bace6fed50548d655b27e735bd07195758580f117e5bc7c6163d6`
for the training report and
`464bc97e61ce8abb3e8ad82a6367754d40a7f13ca4264c285204f55c4f5e6609`
for generation 1 evaluation.
