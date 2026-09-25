# First-root policy-target agreement across search seeds

Status: all three first-root-only cells completed on the same 48 frozen cases.
No learner was trained. This audit changes the root Gumbel seed while keeping
weights, source cases, 16 candidates, and 128 simulations fixed.

## Validity and result

Every first-root state hash and the complete ordered action-witness list
matched the retained baseline across all seeds. Under seed 20260922, all
48 selected first actions also exactly matched the retained baseline. Seeds
20260923 and 20260924 selected a different first action in every case;
95 of those 96 selections lay outside the **original seed's visited 16**.
Across the three seeds, visited-set Jaccard overlap averaged 0.334. This
shows that the 16 candidates are strongly seed-dependent under the frozen
near-uniform priors. It does not say that the other seeds' choices are better.

| Target | Mean seed-pair JS divergence (bits) | Mean mass on unvisited actions | Effective action fraction | Tested-pair rank reversals |
| --- | ---: | ---: | ---: | ---: |
| Current | 0.0021 | 0.9947 | 0.9938 | 606/3,951 |
| Q-logit 2x | 0.0147 | 0.9809 | 0.9586 | 608/3,951 |
| Q-logit 4x | 0.2828 | 0.7158 | 0.3132 | 608/3,951 |
| Half visited | 0.4839 | 0.4970 | 0.0851 | 974/3,951 |

The current target's small divergence partly reflects its nearly uniform
distribution over about 2,734 actions; it is **not** evidence of a stable,
informative selected-action label. Sharpening the target or assigning half
its mass to visited actions removes some dilution but exposes strong
search-seed variation. The 3,951 tested-action pair comparisons share cases
and should not be treated as independent observations. These are root-only
target stability measurements, separate from the prior continuation-seed
outcomes and the candidate-width budget matrix. The
[case-level summary](2026-09-25-hierarchical-target-seed-summary.json)
retains selected action hashes, pairwise visited overlaps, and target JS
values. Offline outcome alignment and a paired learner comparison remain
necessary before attributing training oscillation to target dilution.

## Provenance and closeout

The [protocol](2026-09-25-hierarchical-target-seed-protocol.json) holds exact
panel, configuration, model, binary and runner hashes; the
[progress](2026-09-25-hierarchical-target-seed-progress.json) holds the three
raw-report hashes. The search runner used source commit `d834cc88` and SHA-256
`ab0332710104503d070a3e0a852219088c0744ee4041207447facd9d14b995d0`.
After all searches finished, summary generation failed because a local
dictionary shadowed the output path variable. Commit
`3ef95cc5f8859d478b7067914d44840b7bd34276` fixed only that summary
variable; the final summarizer SHA-256 is
`b0f6369b83b6c5bdf4fa7573b501d3a6e765c703b0f06337f3f1d44efac191e7`.
No search output or analysis formula changed. The three cells took 51 wall
seconds and wrote 240,767,522 raw bytes. No W&B tracking was needed because
there was no training. The
[cleanup manifest](2026-09-25-hierarchical-target-seed-cleanup.json)
records raw removal and disk counts after this package was verified.

With the documented libtorch environment exported under `~/Srv`:

```bash
python3 tools/factory-edit-pilot/hierarchical_target_seed_agreement.py prepare \
  --output artifacts/target-causal-study --experiment-commit d834cc88
python3 tools/factory-edit-pilot/hierarchical_target_seed_agreement.py run \
  --output artifacts/target-causal-study
python3 tools/factory-edit-pilot/hierarchical_target_seed_agreement.py summarize \
  --output artifacts/target-causal-study
```
