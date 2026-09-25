# Frozen-root target contrasts against replicated action outcomes

Status: offline scoring completed on the frozen generation-87 search records.
This stage did not train a learner or change search. The target learner protocol
uses its predeclared selection gate.

## Method and result

The three retained 16-case, 16-candidate/128-simulation baseline collections
provide the same first-root actions and later roots for all four target
variants: current, Q-logit 2x, Q-logit 4x, and half of mass on the conditional
visited target plus half on the original mixed prior. All transforms were
normalized on the full action list. First-action outcome scoring used only
the 66 action pairs whose direction repeated by at least two of three
completions in **both** independent downstream seed triplets. The pairwise
logistic loss uses the two tested action probabilities; their common
normalizer cancels. The 22 cases with confirmed pairs, rather than individual
correlated pairs, are the units for the overall mean.

| Target | Case-mean pair loss, lower is better | First-root unvisited mass | Later-root unvisited mass | First-root effective action fraction |
| --- | ---: | ---: | ---: | ---: |
| Current | 1.886 | 0.9947 | 0.6451 | 0.9938 |
| Q-logit 2x | 3.427 | 0.9810 | 0.5386 | 0.9594 |
| Q-logit 4x | 6.611 | 0.7251 | 0.2252 | 0.3244 |
| Half visited | 2.146 | 0.4970 | 0.3201 | 0.0852 |

No alternative improved the case-mean loss by the predeclared 0.05 nat gate.
The half-visited variant improved loss in S2 but worsened it in S1 and S3;
Q-logit 2x and 4x worsened all three streams. The
[full analysis](2026-09-25-hierarchical-target-contrast-analysis.json)
records case-level losses, winner masses and ranks, first/later target
statistics, and conditional target rank agreement across the candidate-width
matrix. The first-root target is indeed highly diluted, but simply sharpening
it made these externally checked action outcomes less likely. A higher
average mass on a confirmed winner does not imply better pairwise scoring:
concentrating mass on a wrong action can incur a large loss.

The frozen-root result is an association with a small, selected action set,
not evidence that the current target trains best. It cannot separate target
learning from on-policy search feedback or measure retention. The protocol
therefore selects `half-visited` as a **stress contrast only**, with no
outcome-alignment or promotion claim. The next paired learner screen compares
first-root and all-root application from identical source reports and weights.

## Provenance and closeout

The analysis JSON hashes the candidate-width progress, primary continuation
summary, and independent replication summary. Input report SHA-256 values are
in the retained [candidate-width progress](2026-09-25-hierarchical-candidate-width-progress.json)
and original continuation protocol. Source analysis code from commit
`36aae7eaa7421a67f6634a8fd99729b533d8f2d4` has SHA-256
`c6e0963ec3013fc2ebe72cb20d2c3b993fa06a2671d0af3d0ee8328566fc5b81`.
The exact analysis JSON SHA-256 is
`e74deb02ec00aafe7e5fed11d4828297dd26eacecf67d59bd0ce36dece4774b9`.
The earlier release binary and checkpoint identities are unchanged. Offline
analysis took approximately one second; W&B tracking was not needed. The
analysis JSON and three frozen collections remain explicitly needed for the
learner screen; the small disposable log is removed in the
[cleanup manifest](2026-09-25-hierarchical-target-contrast-cleanup.json).

```bash
python3 tools/factory-edit-pilot/hierarchical_target_contrast.py \
  --output artifacts/target-causal-study
```
