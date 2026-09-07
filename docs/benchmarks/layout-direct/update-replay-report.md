# Reproducible update gate result

Status: gate passed for the declared reference cases and two real main-path updates.
Date: 2026-09-07. No learned efficacy or production readiness claim.

## Implemented contract

The streaming trainer now records every training action's pre-action observation, conditional
decisions, legal support and behavior probabilities. The original combined backward expression,
accumulation order, norm-1 clipping and single Adam step are isolated in `update.py`; the old
inline update is removed. One implementation now serves training and offline reproduction.
The network, masks, sampler, credit and executor are unchanged. No obsolete-record adapter is
provided: old trajectories without behavior/optimizer evidence cannot pass this verifier.

Each group retains a finite JSON capsule with deduplicated observation states, the original
root artifact, executed episode references, full credit, model/Adam/RNG snapshots, Python and
environment/catalog/binary provenance, gradient fingerprint and post-update conditional KL/TV.
No pickle is used. CPU precision, MKLDNN, determinism and thread settings are explicit.
The verifier reconstructs pre-action score normalization/futility from the executed episode,
checks source/root identities and prior audits, and reproduces the optimizer state and model.
This is exact single-update replay, not environment reexecution or full training-job resume.

## Predeclared main-path check

`audit-smoke.json` executes two groups of four 32-action trajectories: a freshly generated
seven-facility mixed-transport factory with an active two-node cycle, then the explicit
59-facility target. One new five-facility development root and the target receive initial,
uniform and trained 32-action controls. All 256 training and 192 evaluation actions execute;
all 14 episode outputs pass fresh geometry and original full-factory coverage audits.

The final run is `target/layout-direct-update-audit-02`, completed in 6.3684 seconds. This was
not isolated from other local checks and is not a throughput benchmark. Evaluated scores tie
across the three policies; the run was designed to test persistence, not learned advantage.

| Group | Capsule SHA-256 | Bytes |
| --- | --- | ---: |
| 1 | `444455d32a91864944278feabba393081f891634f13aa0fa32cfe8d44cef0546` | 5,473,519 |
| 2 | `b746b3e6303ae14399cef77a234436c9a5b59aceb249bc8b588b12eb0b2b4789` | 22,398,998 |

Both capsules reproduce in separate processes, including the second update's nonempty Adam
moments. Their pre-clipping gradient hashes are respectively
`0842041517fdc42b611f866918c06d604e24ed9d9380b58d77db2c3ff80ac3de` and
`e2c7e0fdc5bc33f73889111206d5c4310be7f72c6d0bdbda1cba026cc3e7a10a`.
All eight decoder stages appear in both groups. Distribution diagnostics concern recorded
old-policy prefixes only, not full trajectory KL or a causal explanation of learning quality.

Post-rollout snapshot/packing/distribution/serialization work takes 0.2731 and 0.8992 seconds.
Decoder trace construction takes 0.0066 and 0.0171 seconds **inside** reported inference time;
update trace/comparison/gradient fingerprinting takes 0.0163 and 0.0275 seconds **inside**
reported update time. Small behavior-record assembly costs are also reported. Do not add
these subsets to their parent times twice. These measured regions are not an isolated on/off
overhead benchmark. Nearly 28 MB for 256 actions warrants storage measurement before scaling;
the 512 MiB capsule guard rejects rather than truncating evidence.

## Independent review and verified fixes

An independent read-only review found four evidence-contract gaps after the preliminary
`update-audit-01` check. Locally, the old verifier accepted a root reduced to its ID and an
altered unused `initial_score`; environment/feature sources and backend precision flags were
absent. Trace overhead was included in inference/update but not separately attributed.

Accepted fixes: bind the complete root to a hashed artifact and recompute score context;
include existing worker environment and generation provenance; report embedded timing subsets;
record/check backend precision/determinism. Corruption tests now cover these cases. The first
run remains developmental evidence, not a final-format replica. The final run repeats the
same preregistered seed and budget after those fixes, not a search for better performance.
Both runs produce identical gradient fingerprints and training/evaluation score outcomes.

## Verification

- Sixteen Python tests pass: exhaustive credit references; instrumentation preserves selected
  actions, RNG, scalar loss and gradients; independent legacy combined arithmetic matches;
  JSON replay includes nonzero Adam moments; altered behavior/credit/state/root/context/source/
  precision/checkpoint records are rejected.
- Fifteen direct Node tests pass; workspace check and tests pass (two existing ignored tests).
- Both `replay.py` CLI invocations return `exact-update-reproduced`, 128 actions each, with
  `environment_reexecuted: false` and `efficacy_established: false`.
- No Rust or production semantics change. Existing user-owned edits are preserved.

```sh
uv run --frozen --project docs/benchmarks/layout-learning python docs/benchmarks/layout-direct/train.py \
  --plan docs/benchmarks/layout-direct/sampling.json --config docs/benchmarks/layout-direct/audit-smoke.json \
  --output-directory target/layout-direct-update-audit-02
uv run --frozen --project docs/benchmarks/layout-learning python docs/benchmarks/layout-direct/replay.py \
  --capsule target/layout-direct-update-audit-02/update-0001.json \
  --sha256 444455d32a91864944278feabba393081f891634f13aa0fa32cfe8d44cef0546
uv run --frozen --project docs/benchmarks/layout-learning python docs/benchmarks/layout-direct/replay.py \
  --capsule target/layout-direct-update-audit-02/update-0002.json \
  --sha256 b746b3e6303ae14399cef77a234436c9a5b59aceb249bc8b588b12eb0b2b4789
```

Use a fresh output directory to run training again, and read its new hashes from group reports.
Timing metadata makes fresh capsule bytes differ even when optimization reproduces exactly.

## Next gate

The user's requested qualitative inspection is recorded in `trajectory-inspection-report.md`.
It exposes an actually useful multi-action sequence with zero current rank credit, not merely
a high rejection statistic. Next design fixed-policy repeated-batch task/entropy gradient and
credit ablations, then fixed checkpoint quality curves. Do not increase model size or training
budget on the strength of this plumbing check. The broader construction-policy proposal remains
unapproved and separate from these learning diagnostics; the original product goal remains open.
