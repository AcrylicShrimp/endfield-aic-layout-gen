# Integrated I/O readiness: contracts pass, expanded learning not cleared

The corrected I/O pipeline passes its integrated correctness, ownership, recovery,
resource and split checks. The fixed four-arm cold comparison also completed.
Its corrected-minus-baseline quality delta is **-1.129 percentage points**, so the
bounded screen does not clear long training or pretraining. This result does not
restore any known-broken I/O behavior.

## Verdict

| Question | Result |
| --- | --- |
| Are the corrected I/O and learning contracts integrated? | **Yes** |
| Did all four arms finish their fixed 32 updates and 32,768 samples? | **Yes** |
| Are the actual admitted starts complete and disjoint from evaluation roots? | **Yes**, 512 starts per arm, zero overlaps |
| Did normal checkpoint resume preserve nonempty learning state? | **Yes** |
| Did corrected quality beat the immutable baseline in this screen? | **No**, -1.129 pp |
| Is expanded training cleared? | **No** |

The result is a bounded two-initialization screen. It is sufficient to reject an
automatic expansion, but it does not identify a single causal repair or establish
long-run learning behavior.

## Cold quality comparison

Each arm used 32 streams, 32 decisions per update, 64-decision episodes, 32
updates, width 40, two message rounds and four PPO epochs. Evaluation used the
same immutable 16-root panel with two replicates per root and only update 32.

| Arm | Equal-strata mean | Small mean / improvement rate | Large mean / improvement rate |
| --- | ---: | ---: | ---: |
| Baseline, seed 71 | 23.958% | 42.361% / 100.0% | 5.554% / 75.0% |
| Corrected, seed 71 | 26.932% | 41.363% / 87.5% | 12.500% / 87.5% |
| Baseline, seed 113 | 23.784% | 39.645% / 100.0% | 7.924% / 75.0% |
| Corrected, seed 113 | 18.553% | 30.439% / 81.25% | 6.666% / 87.5% |
| **Two-seed mean, baseline vs corrected** | **23.871% vs 22.742%** | **41.003% vs 35.901%** | **6.739% vs 9.583%** |

The corrected arm gained 2.974 pp at seed 71 and lost 5.232 pp at seed 113.
Across initializations, the small stratum lost 5.102 pp while the large stratum
gained 2.844 pp. The required aggregate weights the two strata equally, yielding
the final -1.129 pp screen.

All 128 configured critic epochs completed per arm. The actor completed 122, 119,
127 and 122 epochs respectively because the existing KL early-stop rule ended a
few actor epochs; this is configured PPO behavior rather than an incomplete arm.

The comparison remains strictly matched. Seeds 71 and 113 have respectively 429
and 432 common fresh slots, with no unmatched slot. Each arm's retained odd
checkpoints reconstruct exactly 16 waves x 32 starts. Offline canonical physical
and root auditing finds no overlap with the evaluation panel.

## Throughput and retained optimizations

All established optimizations are present in both applicable paths: persistent
native workers, packed graph/action inference, distributed masked placeholder
references, one-minibatch-ahead PPO preparation, foreach Adam, sparse selected
replay, validated-input reuse and atomic direct checkpoint restore. The readiness
harness adds diagnostic checkpoint retention; it does not patch production or
baseline training behavior.

| Arm | Training wall time | Samples/min | Retained bytes | Peak update RSS |
| --- | ---: | ---: | ---: | ---: |
| Baseline 71 | 69.109 s | 28,449 | 1.687 GB | 6.048 GB |
| Corrected 71 | 83.395 s | 23,576 | 2.879 GB | 7.758 GB |
| Baseline 113 | 74.531 s | 26,379 | 1.709 GB | 6.221 GB |
| Corrected 113 | 89.568 s | 21,951 | 3.020 GB | 7.069 GB |
| **Combined rate** | **71.820 vs 86.481 s mean** | **27,375 vs 22,734** |  |  |

Corrected wall time is 20.41% higher and throughput is 16.95% lower. Both
initializations show the same wall-time increase: 20.67% and 20.18%. This crosses
the plan's 20% investigation trigger.

Phase means localize the added work. Admission falls 3.7%, while collection rises
24.8%, PPO rises 41.3% and checkpoint publication rises 35.0%. PPO preparation
worker time rises 66.4%, but overlaps learner work and is not added again to wall
time. Final replay payloads are 0.87-0.89 GB in the corrected arms versus
0.24-0.30 GB in the baseline arms. The direction is consistent with the required
wider physical graph features (41/37/15 rather than 24/14/7) and terminal-bound
nonlinear prefix being carried through collection, learning and diagnostic saves.
This gate does not isolate their exact shares, so further performance work should
profile those three phases instead of removing the required contracts.

These 32-stream measurements include unusually dense evidence retention and are
not directly comparable with the earlier optimized 128-stream rates of 33,846
fresh or 64,326 cached samples/minute.

## Correctness and recovery evidence

The explicit native matrix covers seven cases and 12 `Runtime::step` calls. It
records nine charged decisions, two selection errors and one uncharged closed-
session reply. Improved, accepted-worse, unfinished, unchanged and rejected
outcomes remain distinct; native route/preflight work reconciles; resource stops
do not become geometry labels; and only the true two-decision horizon terminates
an episode.

The retained small CUDA gate completed four updates and 256 fresh transitions,
including four critic epochs per update, two positive replay sequences, two labels
and 64 replay uses. Checkpoints 1 and 3 recover all four actual admitted starts
after ordinary input pruning. Removing a required checkpoint yields the expected
incomplete-evidence rejection. The existing exact next-update test covers replay,
Adam and RNG continuation against uninterrupted execution.

The scientific final-71 arm intentionally stopped at update 3 and resumed through
update 32. Final-113 was stopped at update 22 by the existing 16 GiB free-space
guard. Inspection found 15,525,433,827 bytes in its isolated regenerable
`target/debug` build cache. Removing that exact cache restored free space; no
experiment data was removed. The arm then resumed under the same registered
allocation and clock. The pre-recovery result and recovery registration remain
retained. A release rebuild can recreate the removed cache.

The complete allocation used 327.167 of 1,800 seconds. It retained 40,018,170
setup bytes and 9,456,950,468 post-publication scientific-root bytes, for
9,496,968,638 combined bytes and the same combined peak, below 24 GiB. Peak process-tree RSS was
7,937,150,976 bytes, below 48 GiB. All owned processes exited and 31,750,643,712
filesystem bytes remained free at closeout.

The earlier 4 GiB attempt remains useful preflight evidence. Its measured small
run projected 17,330,331,648 four-arm bytes and therefore correctly prevented any
cold arm. The authorized 24 GiB run reused and revalidated both setup roots rather
than rerunning the matrix, panel or small gate. Actual combined retention was
9.50 GB; the projection was conservative.

## Provenance and reproduction

Baseline revision is `83f4dd8`; the corrected production revision is `5fa1586`.
The registered binaries, catalogs, templates, setup roots, panel hashes, final
updates, checkpoint manifests and admission audits are recorded in the companion
[compact evidence manifest](2026-09-15-integrated-io-readiness-evidence.json).
The complete raw result is retained at:

```text
/home/ashrimp/Srv/aic-learning-io-20260915/artifacts/learning-io-readiness-scientific/result.json
```

Run a reproduction only in a new artifact root:

```sh
node --test tools/native-training/io-readiness.test.mjs
node tools/native-training/io-readiness.mjs \
  --config tools/native-training/io-readiness.json \
  --output /new/ignored/artifact/root
```

`io-readiness-recover.mjs` is a narrow continuation command for a registered
partial result after a removable host-space blocker. It preserves the original
elapsed clock, invocation ledger and resource accounting and refuses unrelated or
already complete results. It is not a general way to extend an allocation.
