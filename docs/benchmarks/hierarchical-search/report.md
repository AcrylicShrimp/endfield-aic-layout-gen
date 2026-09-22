# Stable-state hierarchical search baseline

The frozen held-out panel compares model-independent beam search with the first
learned receding-horizon MCTS. Every macro child is a native placement command
or a complete route witness, and every reported completion passed the full
independent layout validator.

- Source: `50f8ba41aadff7638f1e6c7be4af8f8b2598ab8b`
- Device: NVIDIA GeForce RTX 5090, Torch `2.13.0+cu130`
- Lower checkpoint:
  `d88f5c1e7b9c49b0bc9f57acdf1cd7e63aaf0f1d93e4330ff3b28a51c0e80b1b`
- Outer checkpoint:
  `32fb8174f159b293ce21d6e0592f35808a953ceb55ab91ae82f599fd9639df75`

| Search | Fully validated | Macro samples/min | Native rejections |
| --- | ---: | ---: | ---: |
| Beam | 4/4 | **86,521** | 0 |
| Learned MCTS | 4/4 | **98,995** | 0 |

MCTS performed 128 simulations per case and committed two macro routes per
case. The learned outer scorer therefore preserved correctness and exceeded
beam throughput on this route-only panel. The result does not exercise facility
placement or shared-network macros.

The retained raw report SHA-256 is
`a9aabb10924e283e592a376f8f28cd62257800340ea5d9dbeeeeff37f5dc25a8`.
