# Hierarchical route guidance warm start

This bounded experiment checks whether learned directed costs can preserve a
corridor needed by a later route while the exact router remains the only path
constructor and native replay authority.

## Frozen inputs

- Source: `a5ae93bafd180e27ebb494646631e4dfa2a65ba0`
- Catalog identity:
  `6e483b5b903fe759008823f50dfe55d6e9a7b3d8d36978864fb292a76d5684de`
- Config SHA-256:
  `7286cd33416973f96bf614e37365ac262d0403e0497493c45544434987581e5f`
- Training panel SHA-256:
  `a685d209d6973f6e2e70dff7cdd84ab62f2c23b81927876340cc183f8aab7ae6`
- Held-out panel SHA-256:
  `ba89cb7997a03762dc91dc964b42a8d8dfa60fa84688601459aa1e62396edec6`
- Device: NVIDIA GeForce RTX 5090, Torch `2.13.0+cu130`, CUDA 13.0
- CPU data workers: 8

The training partition contains 18 translated forward and reverse
escape-corridor states. The held-out partition contains four unseen reverse
translations. Every state has six native-valid root candidates, but only a
subset leaves the only later facility-port escape usable. Partition identities
and case IDs are disjoint.

## Command

```bash
env LIBTORCH="$HOME/Srv/aic-rust-learning-20260915/.venv/lib/python3.12/site-packages/torch" \
  LIBTORCH_CXX11_ABI=1 \
  LD_LIBRARY_PATH="$HOME/Srv/aic-rust-learning-20260915/.venv/lib/python3.12/site-packages/torch/lib" \
  RUST_MIN_STACK=8388608 \
  target/release/aic-cli learning train-route-guidance \
  --config docs/benchmarks/hierarchical-route-guidance/config.json \
  --training-panel docs/benchmarks/hierarchical-route-guidance/training-panel.json \
  --evaluation-panel docs/benchmarks/hierarchical-route-guidance/evaluation-panel.json \
  --catalog-root data/game/normalized \
  --output artifacts/hierarchical-route-guidance-commit-a5ae93b \
  --device cuda:0
```

A sample is one native-valid complete route candidate with an exact downstream
continuation label. The 400 epochs processed 43,200 samples in 6.121 seconds,
or **423,491 samples/min**. Loss decreased from 19.849 to 0.480. The maximum
pre-clipping gradient norm was 119.639 with a configured clip of 1.0.

## Held-out result

| Measure | Before training | After training |
| --- | ---: | ---: |
| Anchor full later-route survival | 0/4 | 0/4 |
| Model-selected full survival | 0/4 | **4/4** |
| Exact-best selections | 0/4 | **4/4** |
| Mean selected continuation quality | 0.000 | **1.000** |
| Oracle mean continuation quality | 1.000 | 1.000 |

Fresh guided exact searches returned 24 native-valid root candidates. Twenty
came from guided or guided-diversified queues, four of those preserved the later
route, and every held-out case had at least one such candidate. Native replay
rejected zero candidates. The non-learned anchor remained present and failed in
all four cases, so the result measures learned corridor guidance rather than an
anchor improvement.

The retained server checkpoint SHA-256 is
`d88f5c1e7b9c49b0bc9f57acdf1cd7e63aaf0f1d93e4330ff3b28a51c0e80b1b`.
The ignored local raw report SHA-256 is
`3622082f2cc783f5e1a48ee36408d9c2f00c967dca41ec9344117c1db7e60dea`.
This small panel establishes the lower-boundary behavior; it does not establish
full-factory generalization or shared-network legalization.
