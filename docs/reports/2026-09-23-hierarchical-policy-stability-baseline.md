# Hierarchical policy stability: diagnostic baseline

The fixed 64-case train-source diagnostic panel was evaluated once with the
generation-87 lower and outer weights that seed all four screen arms. No
training occurred, and none of the cases overlap the eight predeclared
training panels by problem identity. The old validation panel was not read.

The starting models completed **27/64** cases. This is the common reference
for paired gains and losses in the eight-generation screen; its score is not a
hyperparameter target. For context only, the short smoke's generation-1 A
model completed 22 cases, gaining 10 and losing 15 relative to the start; D
completed 29, gaining 16 and losing 14. Even the higher total masks substantial
case replacement, which is why the screen uses paired case changes and the
last two evaluations rather than one checkpoint's count.

The compact
[`baseline evidence`](2026-09-23-hierarchical-policy-stability-baseline-evidence.json)
contains all 64 case outcomes and the panel, raw report, initial checkpoint
and source identities. Its source commit is `31555f5`. From the isolated server
source under `~/Srv/aic-policy-stability-20260923`, with LibTorch configured as
in [`training-server.md`](../maintenance/training-server.md), the command was:

```bash
target/release/aic-cli learning hierarchical-search-harness \
  --config docs/benchmarks/hierarchical-policy-stability/search.json \
  --panel artifacts/policy-stability-inputs/diagnostic-panel.json \
  --lower-training-output artifacts/policy-stability-inputs/initial-lower \
  --outer-training-output artifacts/policy-stability-inputs/initial-outer \
  --catalog-root data/game/normalized \
  --output artifacts/policy-stability-inputs/baseline-report.json --device cuda:0
```

The complete root-record report was used only to extract the case outcomes.
After checking the compact evidence, the raw report and log were removed; the
[`cleanup manifest`](2026-09-23-hierarchical-policy-stability-baseline-cleanup.json)
records 224,600,824 file bytes removed and free space increasing from
177,158,262,784 to 177,382,875,136 bytes. The
selected panel and initial model inputs remain needed for the screen.
