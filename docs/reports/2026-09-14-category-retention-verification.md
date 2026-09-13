# Bounded category-retention verification

> Withdrawn from scientific selection on 2026-09-14 at the user’s request.
> Historical record only; use the [replacement quality study](../designs/2026-09-14-quality-weighted-replay-study.md) for new selection evidence.

The opt-in `declarative-replay-category-v2` implementation passed its mechanism
checks. It is available for experiments; the production configuration remains v1.
This report does not establish improved learning performance.

## Implementation and continuity

The shared replacement index preserves prospective-stratum selection, mature
utility ordering, token invalidation and one index build per update. Three fixed
category/age groups implement temporary protection without a per-victim scan.
Capacity overrides and protection counts are explicit in v2 eviction events.
The pure geometry implementation is shared with the existing scorer. Features
are extracted at terminal admission only for a configured consumer, checkpointed,
and validated before restoration hydration. Active duplicates retain their age.
Normal v1/uniform admission does not extract geometry or add metadata.

## Evidence

- On server `ashrimp-arch`, the unchanged active-source v1 and new v1 processed
  the same two recorded native-fixture updates with nonempty Adam state, actual
  replay and an eviction. Model, both optimizers, all checkpoint RNGs, full buffer,
  ordered events and selected IDs matched exactly.
- Seven new focused tests pass, including 1,500 randomized indexed/oracle choices,
  grace expiry, capacity override, q=0 v1 equivalence, metadata rejection, actual
  rotated place success, duplicate age, snapshot/restore and victim cache removal.
  The 4,096-entry warm-up visits two source chunks in two groups, with no repeated
  source-order traversal. Mixed label lengths remain within both hard capacities.
- Existing declarative Python suite: 43 tests passed (one Linux-only transport
  test skipped on macOS). Existing scorer: ten tests passed after factoring its
  unchanged math. `cargo check --workspace` and `cargo test --workspace` passed;
  three pre-existing native external-fixture/oracle tests remain ignored.
- Actual diagnostic CLI accepts v1 and v2. Invalid category fraction 0.21 exits 2
  with structured code `declarative.replay-retention.input`.

The bounded server microbenchmark uses five fixed repetitions, each with 4,096
entries and 256 removals/admissions, one index construction and zero source loads:

| Policy | Median seconds |
| --- | ---: |
| v1 | 0.003261 |
| v2, capped exterior comparator | 0.007055 |

The additional 3.795 ms includes feature validation and category heaps. This
small absolute index cost does not measure feature extraction, policy inference,
publication, or retained bytes; the registered integrated readiness step covers
those costs before any recommendation.

## Reproduction and provenance

Server working directory:
`/home/ashrimp/Srv/aic-retention-experiments-20260914`.
The Python executable is the existing environment under
`/home/ashrimp/Srv/aic-ppo-padded-curriculum-12h-20260911/tools/factory-edit-pilot/.venv`.

```sh
AIC_NATIVE_LIBRARY=/home/ashrimp/Srv/aic-declarative-batch-20260913/target/release/libaic_pilot_native.so \
  python tools/factory-edit-pilot/diagnostics/declarative_retention_verify.py \
  --config artifacts/retention-verification/v1.json \
  --output artifacts/retention-verification/v1-result.json \
  --baseline-runtime /home/ashrimp/Srv/aic-declarative-storage-20260914/tools/factory-edit-pilot
```

Use `--validate-only` for v2/invalid configuration checks without repeating the
measurements. Raw results and exact configurations are retained locally under
`artifacts/retention-experiments/20260914/retention-verification/` and in the server
working directory's `artifacts/retention-verification/`.

Verification config SHA-256: `d469865acf7357a6ec59f64b59a5e9a3bef204e0e26c40998eb36477bfd8a45c`.
Exact paired-record digest: `77ad0ada204671f658650f66076a61bb388963ae177bcfe9ee0d4ffdcac33c22`.

Main training was gracefully paused at update 1744 with
24022.639999 accumulated training seconds before the first
server experiment. The complete-wave checkpoint, worker release and free run lock
were verified. Parent checkpoint SHA-256:
`e09bce8cd3d32bab3ccfc3652825e263cc48f2a24ae396f041a0af6b85486cc4`.
Main training must remain paused after all experiments.
