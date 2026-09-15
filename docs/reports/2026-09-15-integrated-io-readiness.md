# Integrated I/O readiness: correctness passes, comparison allocation blocked

The corrected I/O contracts and the small native learning/recovery gate pass.
The four cold comparison arms **have not started**. The diagnostic retention
preflight blocks their allocation under the approved 4 GiB output ceiling.
There is no final quality difference or comparable production throughput result.
Long training and pretraining remain **not cleared**; this does not reverse the
completed I/O corrections.

## What passed

Plans 00–05 establish the single learned version boundary, translation-invariant
working frame, authoritative physical inputs, complete terminal-bound actions,
available-family priors and canonical physical/root reservations. Their retained
CPU/CUDA ownership, likelihood, masked-gradient and exact next-update checkpoint
evidence remains applicable. The complete learning-enabled workspace tests passed
again with the final native diagnostic helpers, including the nonempty replay,
Adam and RNG continuation test. Default workspace checks/tests also passed.

The explicit ignored `integrated_io_readiness_matrix` test exercises seven native
cases and 12 `Runtime::step` calls. It records actual replies once per case;
there is no normal-training journal or per-action disk output.

| Evidence | Observed |
| --- | --- |
| Charged decisions / selection errors / uncharged closed-session reply | 9 / 2 / 1 |
| Improved / accepted worse / unfinished / unchanged / rejected replies | 1 / 1 / 3 / 3 / 2 |
| Native route / preflight expansions | 2,611 / 1,820 |
| Route calls / complete-candidate validations | 9 / 3 |
| Exact work exhaustion | One expansion consumed; unfinished draft and valid best retained |
| Wall cancellation | No charged decision; no geometry label |
| Current versus best | Accepted worse keeps its actual metrics; unfinished states have no current acceptance metrics |
| True horizon | First decision remains live; second completes the configured two-decision episode |

The matrix disables band masking to exercise a native rejected band. Scientific
arms retain structural masking. Complete-candidate validation failure uses an
explicit post-admission request fault injection confined to the test. Censored
resource stops and that fault are distinguished from true episode completion;
they are not invented policy outcomes or baseline behavior counters.

A new 16-root panel (eight small/eight large, seed 9015071) was prepared with the
immutable baseline and migrated to v2. Both manifests retain identical actual
input hashes and action seeds. No confirmation case was selected by its score.

The actual CUDA small gate completed two waves, two streams and four updates:
**256 fresh transitions**, four PPO critic epochs per update, two positive replay
sequences/two labels and 64 replay uses. After ordinary input pruning, kept
checkpoints 1 and 3 recovered all four actual admitted starts. Removing a kept
checkpoint produced the required incomplete-evidence rejection. The offline new
identity audit validated all four actual inputs and their roots, with zero panel
overlaps. This checks the evidence route; it does not stand in for auditing the
2,048 starts required by the four scientific arms.

## Why the comparison did not start

The small run retained **33,848,304 bytes**:

| Payload | Bytes |
| --- | ---: |
| Immutable sparse replay chunks at updates 1 and 3 | 28,153,244 |
| Checkpoint JSON, including pending replay and live sessions | 3,929,107 |
| Three learner snapshots | 1,543,116 |
| Current input files | 215,557 |
| Update reports and pointers | 7,280 |

Checkpoint interval 1 saves active, unfinished replay at each mid-wave boundary.
Each kept odd checkpoint needs those references. Later successful labels can
still reference the same immutable chunk, so its other serialized rows cannot
be removed by ordinary whole-file pruning. In the small run, the final live
buffer contains only two labels and reports 300,484 bytes, while its two retained
chunk files still contain the earlier pending inputs.

This is the deliberate diagnostic cadence's cost. Normal configuration still
saves every two updates, after full-wave positive selection prunes unsuccessful
preparation. No persistence hook, duplicated normal snapshot, lost batching or
new per-step validation was introduced. The four-arm design deliberately overrides
that cadence to preserve every actual admitted source without modifying baseline
code. Retaining fewer odd checkpoints would break its scientific evidence route.

The final preflight estimate scales all dimensions:

```text
33,848,304 bytes × (32 / 2 streams) × (16 / 2 waves) × 4 arms
= 17,330,331,648 bytes ≈ 16.14 GiB
```

This is a planning estimate, **not a measured four-arm output or certified bound**:
geometry, replay selection, checkpoint overhead and later cleanup affect it.
The initial preflight code used an insufficient factor of 128; even that estimate
(4,332,582,912 bytes) exceeded the 4 GiB ceiling and stopped before any cold arm.
The tool now uses the correct factor of 512, covered by a unit check. No workload
was rerun to validate this arithmetic correction.

A concrete next allocation is **24 GiB retained output**, keeping 48 GiB RSS,
exactly four cold arms/131,072 transitions, the same seeds/panel/budgets/epochs,
and a new 1,800-second execution ceiling. Reuse the retained panel and completed
small-gate evidence. This proposal is **not authorized or executed**. It requires
an explicit allocation decision and a reviewed plan amendment. An alternative
within 4 GiB would need a newly reviewed evidence-retention design; silently
skipping snapshots or changing training size is not an option.

## Runtime and optimization continuity

All established optimizations remain: persistent native workers, packed
multi-graph/root/port inference, distributed masked placeholder references,
one-minibatch-ahead PPO preparation, foreach Adam, sparse selected replay,
validated-input reuse and atomic direct checkpoint restore. Neither baseline nor
production code was patched by this diagnostic.

The small CLI run takes 2.268 seconds: admission 0.234, collection 1.104,
PPO 0.575 and checkpoint publication 0.189 seconds. It reports 6,774 samples/minute
and 2.467 GB peak process RSS; the outer process-tree sampler observes 2.559 GB.
Those 256 samples include initialization and only two streams. They are **not a
throughput comparison** with the earlier optimized 128-stream fresh-input rate
of 33,846 samples/minute, or its 64,326 cached-input rate.

The planned 32-stream comparison must still measure matched end-to-end cost,
including generation, evaluation and diagnostic retention. PPO preparation-worker
time overlaps learner execution and must not be added again. An unexplained
20% end-to-end regression remains an investigation/adoption blocker. No such
regression or improvement has been measured by this gate.

## Execution, provenance and recovery

The first small configuration was rejected before training because its 256-row
minibatch exceeded its two-stream, 64-row chunk. The helper was corrected to 64.
The completed matrix and actual panel were reused in a new output root, carrying
forward the first attempt's clock and output accounting. No learning samples
were duplicated, no panel was redrawn and no budget was extended. The resumed
preflight stopped at 133.317 seconds on the original clock, after 2.717 seconds
for the actual small CLI invocation. Subsequent bounded identity auditing uses
those retained inputs, not generated replacements.

Research host roots:

```text
/home/ashrimp/Srv/aic-io-baseline-83f4dd8
/home/ashrimp/Srv/aic-learning-io-20260915/artifacts/learning-io-readiness
/home/ashrimp/Srv/aic-learning-io-20260915/artifacts/learning-io-readiness-run
```

Baseline revision is `83f4dd8`; production implementation is `5fa1586` with only
explicit test/orchestration additions for this step. Binary/catalog/input hashes,
actual checkpoint sizes and verification evidence are in the companion
`2026-09-15-integrated-io-readiness-evidence.json`. Actual checkpoint 3 and final
checkpoint 4, their referenced chunks, all four admitted sources and both panel
views remain retained. All owned training, test and diagnostic processes exited.

The new `io-readiness.mjs` command validates paths and registered binary/catalog/
helper hashes, executes arms sequentially, verifies retained checkpoint hashes,
audits actual current/root identities and reports only final-update evaluations.
Its budget-failure test kills owned descendants and preserves actual evidence.
Missing-checkpoint and descendant-root rejection checks pass. Failed or missing
scientific evidence cannot produce a favorable quality screen.

Use a new artifact root and a newly registered configuration for reproduction:

```sh
node --test tools/native-training/io-readiness.test.mjs
node tools/native-training/io-readiness.mjs \
  --config tools/native-training/io-readiness.json \
  --output /home/ashrimp/Srv/aic-learning-io-20260915/artifacts/learning-io-readiness-reproduction
```

The checked-in 4 GiB allocation retains the preflight blocker. Do not change that
ceiling or restart the four-arm allocation without the recorded decision/review.
