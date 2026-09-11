# Shared PPO capacity comparison

Status: complete. All eight main runs and final audits passed; see the
[findings](../reports/2026-09-11-ppo-shared-capacity.md) and
[compact provenance](../reports/2026-09-11-ppo-shared-capacity-provenance.json).

The user explicitly requested running the wider shared model experiment after
the initial implementation stopped at configuration and unit verification.

## Experiment contract

Run all four width-40 shared combinations of critic history on/off and additional
critic fitting after policy KL stopping on/off. Use the previously declared
width-40 model (86,282 unique parameters), seeds 17 and 29, 32 batches of 64
factory attempts, 32 actions, and evaluations at 0/8/16/24/32 with 32 attempts.
Keep all workload seeds, native binaries, catalogs, PPO settings, action inputs,
retention and resource bounds equal to the completed factorial. Every run starts
fresh. Do not resume the stopped twelve-hour run.
The actor always receives its last 16 actions; the history factor changes only
the critic's input. The actor's best-world input remains hidden in every cell.

The existing eight width-32 cells are retained controls, not newly randomized
concurrent runs. Compare width 40 against the matching shared width-32 and
separate width-32 cells for each history/schedule condition and seed. Verify
matching input geometry and configuration outside the declared model change.
Do not treat model initialization hashes across different widths as equal.
Within width 40, require common initialization across all four cells and exact
joint-update equality through the first critic tail in each stop/tail pair.

Execute sequentially in a separate frozen server source tree. First run bounded
two-batch readiness for both history settings with critic tails forced by a small
KL threshold. Audit native results, CUDA predictions, nonempty optimizer resume
and exact recorded updates before the main screen. Use online W&B for all eight
main runs and verify remote completion/scalars afterward. Keep the original
source trees and irreplaceable checkpoints unchanged.

## Outputs and interpretation

- Versioned four-cell matrix, existing per-run protocols/checkpoints/traces and
  stable harness failure diagnostics. Any failed cell stops subsequent work for
  investigation; no silent shortening or fallback.
- Audited report for all four new cells, two seeds and every evaluation. Include
  native final validation, input/source/checkpoint identities and tail invariants.
- Paired final area-reduction differences against both control architectures,
  critic error, failure fraction, actual training work and elapsed time. Include
  per-seed learning curves and changes from each model's own initialization.
- English findings, compact provenance and reproducible commands committed;
  generated evidence and plots retained under ignored artifacts directories.

Two seeds are exploratory. Each run seed controls both model initialization and
factory/action random draws; compare treatments within the same seed. Reused
held-out factories do not provide an untouched
confirmation set. This is an approximately matched total parameter comparison;
width also changes the actor's capacity, initial function and compute. It cannot
alone attribute differences to gradient interference or guarantee a ranking
under the earlier inherited-start curriculum. Timing also includes changed KL
stopping, visited geometry and non-concurrent machine conditions.
Learning gains from different untrained policies are descriptive; a larger gain
does not by itself establish better final quality or faster causal learning.

## Readiness evidence

Runtime: `/home/ashrimp/Srv/aic-ppo-shared-capacity-readiness-20260911`.
The two-cell matrix completed in 46.46 seconds. All 90 Linux/CUDA PPO tests
passed. Both native audits verified 48 result records and exact nonempty-Adam
continuation at batch two, including one actor epoch and four critic epochs.

Reports are retained under `artifacts/factory-edit-pilot/` in that runtime:

| Report | SHA-256 |
| --- | --- |
| `audit-shared-nohistory-tail/report.json` | `4b80bb3ae2ebfd1784fa6f45cedbf1be52dc51140e872f8e9f23d0492ceb10de` |
| `audit-shared-history-tail/report.json` | `d376080fd1213aaa136bbdffa049cd31ff100a3defde4ba717498b4b751b04a9` |
