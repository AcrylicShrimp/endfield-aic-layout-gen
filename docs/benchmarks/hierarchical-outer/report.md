# Hierarchical outer-model warm start

This bounded CUDA run verifies the separate macro policy/value model and fixes
the pre-distillation throughput baseline.

- Source: `50f8ba41aadff7638f1e6c7be4af8f8b2598ab8b`
- Device: NVIDIA GeForce RTX 5090, Torch `2.13.0+cu130`
- Catalog identity:
  `6e483b5b903fe759008823f50dfe55d6e9a7b3d8d36978864fb292a76d5684de`
- Lower checkpoint:
  `d88f5c1e7b9c49b0bc9f57acdf1cd7e63aaf0f1d93e4330ff3b28a51c0e80b1b`
- Outer checkpoint:
  `32fb8174f159b293ce21d6e0592f35808a953ceb55ab91ae82f599fd9639df75`

The 400 epochs processed 86,400 native-valid macro-action samples in 1.585
seconds, or **3,271,001 samples/min**. Loss changed from 2.690 to 2.389. The
four held-out states all contained at least one action that completed the fixed
continuation; the model selected a completing action in 4/4 states both before
and after this small warm start. This confirms the model and training boundary,
but the panel is too easy to demonstrate policy improvement.

The retained raw report SHA-256 is
`45d4be3c44729f78e70f6b0a113b5decda34a8fe3c849a23ed47cb413efa5a6e`.
