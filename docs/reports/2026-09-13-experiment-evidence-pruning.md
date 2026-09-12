# Selective experiment evidence retention

AIC artifacts now occupy **17.41 GiB**, down from **102.96 GiB**. The measured
directory reduction is **85.55 GiB** (83.1%); the operation-level total is
85.60 GiB before the small retained cleanup manifests and directory overhead.
The final filesystem has **141.47 GiB available**. These totals include the
new legacy archives; no space saving is claimed merely by moving data elsewhere.

The user requested actual cleanup of the 102.96 GiB of AIC experiment artifacts,
not just build caches. The earlier cache cleanup had preserved too much historical
raw experience. Retaining a research conclusion does not require keeping every
sampled action, tensor batch and intermediate checkpoint indefinitely.

## What was reduced

| Operation | Allocated space reclaimed |
| --- | ---: |
| Remove detailed traces from the completed six-arm blueprint evaluation | 22.80 GiB |
| Remove blueprint training chunks outside the retained dependency set | 10.32 GiB |
| Remove old experience/gradient tensors, initially preserving the last two groups | 14.66 GiB |
| Remove remaining completed-run tensors and excess intermediate checkpoints | 13.99 GiB |
| Losslessly compress completed large JSONL histories | 1.75 GiB |
| Verify and archive 19 older experiment trees | 22.09 GiB |

These are measured operations on files, not proposed savings. The JSONL pass
replaced 1,955,935,352 source bytes with 74,714,760 compressed bytes and verified
the decompressed SHA-256 of every replacement before unlinking its source.

Older experiment trees outside the active blueprint experiment and main workspace
were then archived with Zstandard. Each archive passed its checksum test and a
GNU tar byte/metadata comparison against the original tree before removal of the
unpacked copy. Small trees were left in place. Archives remain inside each
project's `artifacts/` directory, so the final artifact total includes them.
This is storage reduction, not a backup or a claim that these historical inputs
are all essential. Compact report/state copies remain directly accessible under
`summaries/`; `retention.json` gives the archive and restoration command.

## Preserved for the next experiment

The active advantage experiment keeps all six final learner checkpoints, including
model parameters, optimizer moments, RNGs and buffer identities. The three centered
parents have 287, 206 and 311 unique replay entries. Every entry's complete source
episode remains available. The five selected standardized successes retain their
complete original episodes, not shortened imitation paths. One final PPO chunk
per arm also remains. This selected dependency set occupies 907 original chunks,
3,731,555,554 apparent bytes, instead of all 3,072 chunks.

All 2,048 actual training inputs, their generation attempt records and the unchanged
32-factory evaluation cohort remain. The 774 evaluation reports retain 24,768
factory-level results, scores, action counts, milestones and timing. The 24,768
corresponding detailed trace payloads were removed. Prior human reports,
experiment configurations, aggregate update records, native binaries and source
checkouts remain.

A post-prune check authenticated 12,064 distinct retained files: 907 training
chunks, six final checkpoints, 774 evaluation reports, 8,192 input payload files,
2,087 generation records and 98 historical cohort sources. This verifies the
registered next experiment's file dependencies; it is not a rerun of learning or
a new scientific result. The active experiment's directory is excluded from the
legacy archive pass.

The remaining footprint is about 5.81 GiB for the active advantage experiment,
4.28 GiB for the main workspace's selected diagnostics/unfinished states, and
7.32 GiB for older archives and small retained experiment trees. None of these
figures establishes a permanent minimum retention requirement.

## Changed archival capability

Full historical optimizer replay and per-action evaluation inspection are no
longer available for deleted payloads. The earlier audits describe verification
performed before pruning. Their existence must not be interpreted as promising
that every referenced raw path still exists. Per-run retention markers and the
[cleanup provenance](2026-09-13-experiment-evidence-pruning-provenance.json)
record these changes explicitly.

For completed earlier experiments, the intermediate checkpoint pass kept the
initial and latest two numbered checkpoints. Unfinished or unclassified tensor
sets were excluded from the final completed-run deletion pass; their surviving
payloads are retained in the archives. Installed environments, application settings,
root-owned model caches and package caches were outside this operation.

## Retention practice going forward

Keep concise findings, reproduction settings and per-factory result tables for
completed comparisons. Keep actual inputs, native witnesses and checkpoint state
when the next experiment directly needs them. Retain selected source experience
and the latest recovery window rather than every historical chunk. Record what
was removed, and do not promise exhaustive replay afterward.

The new replay experiment should retain its specified checkpoints and source
successes, while reducing other completed evaluation traces after publishing their
result rows. Retention must not skip scheduled evaluations, alter policy inputs,
or silently discard an unfinished draft or data referenced by its active buffer.

Full removal/replacement manifests are under ignored
`artifacts/server-cleanup-20260913/` locally and in the server's main workspace.
No generated training output is added to Git by this cleanup.

After archiving, the selected Python environment still imports Torch
`2.14.0+cu130`, reports CUDA available, and loads the selected native
`BlueprintEditSession`. Documentation worktree/index and whitespace checks pass.
