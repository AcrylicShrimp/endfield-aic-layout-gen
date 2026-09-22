# Integrated hierarchical full-factory validation

The held-out physical-v2 case starts from an empty native session. It requires
one facility placement, two boundary-terminal placements and a two-member
same-commodity network before the result can be published. Every macro applies
through the native session, and completion means that the independent full
layout validator accepted the final state.

- Source: `a40e226d9e25b530c5fd4ca0c46b8fba51637581`
- Device: NVIDIA GeForce RTX 5090, Torch `2.13.0+cu130`
- Generation 1 lower checkpoint:
  `0b51d70ee03d05b1aaf7793521807127fde784aac53d6d863c5c1ca285ce19ef`
- Generation 1 outer checkpoint:
  `c1f641905f1fcd3a3611bb1294f10311ad1e28928a5d7fa29dd0f13726213f54`

| Search | Fully validated | Macro moves | Simulations | Macro samples/min |
| --- | ---: | ---: | ---: | ---: |
| Beam | 1/1 | 4 | 0 | **547,671** |
| MCTS | 1/1 | 4 | 16,384 | **381,539** |

Both searches committed the sequence `facility -> supply terminal -> sink
terminal -> network`. Beam generated 128 native-valid network candidates across
32 network tasks. MCTS generated 672 candidates across 374 tasks; 381 candidates
used shared cells. It selected one atomic two-route witness with two shared cells
and no native rejection. Its final witness used 33 route steps and passed atomic
replay and independent validation.

The integrated result also verifies the large-action MCTS boundary. A page may
contain 192 placement actions, so prior-ordered progressive widening lets a
bounded run reach the routing layer instead of spending its simulations on one
mandatory visit to every placement. Before every facility and terminal is
placed, exact one-step native progress supplies leaf values because the retained
outer checkpoint was trained on fully placed route states. Learned policy priors
remain blended with deterministic geometry/progress anchors for action ordering.

The retained raw report SHA-256 is
`b4f7a889f5511a38c2213229a35bd7047a4d37aa3aea87ec9dc7287b966bf018`.
