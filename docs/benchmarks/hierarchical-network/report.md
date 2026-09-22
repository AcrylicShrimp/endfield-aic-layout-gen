# Atomic shared-network macro

This held-out physical-v2 case leaves two same-commodity output obligations
after placing one facility, three boundary terminals and the supply route. The
lower tool must expose the outputs as one atomic network action, respect the
combined rate on every shared port and cell, and publish only a fully validated
layout.

- Source: `fc29ff85a7ed1488f645e9ca8037e53165aa3f8c`
- Device: NVIDIA GeForce RTX 5090, Torch `2.13.0+cu130`
- Generation 1 lower checkpoint:
  `0b51d70ee03d05b1aaf7793521807127fde784aac53d6d863c5c1ca285ce19ef`
- Generation 1 outer checkpoint:
  `c1f641905f1fcd3a3611bb1294f10311ad1e28928a5d7fa29dd0f13726213f54`

The lower tool returned six native-valid network candidates from 125 exact
searches and two batched guidance calls. Four candidates used a shared trunk.
No individual point-to-point macro was exposed for either member obligation.
Beam and MCTS each completed both remaining connections with one atomic macro,
and the independent validator accepted the resulting three-connection layout.

| Search | Fully validated | Macro moves | Macro samples/min |
| --- | ---: | ---: | ---: |
| Beam | 1/1 | 1 | **29,030** |
| MCTS | 1/1 | 1 | **24,747** |

MCTS selected a two-route witness with three shared cells. The generator saw one
native rejection while exploring its bounded internal route candidates; that
witness was excluded before the network boundary. The six returned network
candidates all passed atomic replay. A native unit test also confirms that a
tampered shared-cell count is rejected without changing the originating state.

The retained raw report SHA-256 is
`65cc97c8aaa6795a048bd110d2eece886ad26755b16d25dc51afec649a6fff64`.
