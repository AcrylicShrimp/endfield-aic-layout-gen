# Post-held-out lower/outer model attribution under H12 search

Status: all six prespecified missing cells are complete. This is a mechanism
diagnostic on the reused 64-case training-partition diagnostic, not a new
candidate-selection or promotion test. The integrated H12/H12 held-out verdict
remains unchanged.

## Question and controlled method

The [integrated held-out test](2026-09-26-hierarchical-lower-restored-heldout-result.md)
passed, but the H12-trained final weights did not consistently outperform
H6-trained weights under the same H12 search on the primary diagnostic. Which
model origin contributes to that inconsistency? For each of three final
streams, evaluate both missing lower/outer model combinations under the
unchanged H12 search configuration, root seed 20260922, case IDs, 64-case
diagnostic, and frozen model bytes. The primary study already evaluated the
H6-lower/H6-outer and H12-lower/H12-outer cells under H12 search. No training
or held-out evaluation occurs in this stage.

The [protocol](2026-09-26-hierarchical-lower-outer-cross-protocol.json)
binds the completed primary and held-out summaries, binary, panel, search
configuration, and all 12 lower/outer model identities. The
[progress](2026-09-26-hierarchical-lower-outer-cross-progress.json) retains
six new success masks and report hashes. Runner source commit `251a49d3`.

## Paired results

Each cell gives `completed / lost starting H6 successes` of 64 cases.
Within a row, only the indicated lower and outer checkpoint origins differ;
all evaluations use H12 search.

| Stream | H6 lower/H6 outer | H6 lower/H12 outer | H12 lower/H6 outer | H12 lower/H12 outer |
| --- | ---: | ---: | ---: | ---: |
| s1 | 47 / 4 | 38 / 12 | 47 / 4 | 34 / 12 |
| s2 | 38 / 7 | 49 / 6 | 41 / 8 | 49 / 4 |
| s3 | 45 / 8 | 38 / 6 | 34 / 11 | 36 / 9 |

With the H6 outer model fixed, replacing the lower model changes completion
by `0`, `+3`, and `-11` across streams (mean `-2.67`). With the H12 outer
fixed, the lower effects are `-4`, `0`, and `-2` (mean `-2`). Replacing the
outer model with H12-trained weights changes completion by `-9`, `+11`, and
`-7` with the H6 lower fixed (mean `-1.67`), or `-13`, `+8`, and `+2` with
the H12 lower fixed (mean `-1`). The interaction terms are `-4`, `-3`, and
`+9` (mean `+0.67`). The [summary](2026-09-26-hierarchical-lower-outer-cross-summary.json)
contains all four success masks, retention counts, conditional effects, and
six new runtimes. New evaluations average `189.56 s` each.

## Interpretation

The outer-model origin is the largest, sign-changing contrast in s1 and s2:
H12-trained outer weights hurt s1 and help s2 under the same H12 search. In
s3, H12-trained lower weights hurt strongly with the H6 outer model. Neither
model component has a consistent positive completion effect across streams.
This explains why a large H12/H12-versus-H6/H6 completion gain does not prove
that the learned weights steadily improve: the integrated gain can come from
the search horizon even when model-origin effects vary or regress.

This factorial comparison is conditional on checkpoints produced by different
on-policy trajectories. Swapping their components can create combinations
they were not trained together to handle. It identifies effects of the fixed
checkpoint bytes on this diagnostic, not a general causal effect of the
training algorithm or a new production recipe. Any alternative combination
would require its own fresh on-policy training and untouched held-out test.

## Reproduction and closeout

With the parent final checkpoints and fixed inputs under
`~/Srv/aic-target-causal-20260925` on `ashrimp-arch`:

```sh
python3 tools/factory-edit-pilot/hierarchical_lower_outer_cross.py prepare \
  --output artifacts/recovery-lower-restored-v2
python3 tools/factory-edit-pilot/hierarchical_lower_outer_cross.py run \
  --output artifacts/recovery-lower-restored-v2
python3 tools/factory-edit-pilot/hierarchical_lower_outer_cross.py summarize \
  --output artifacts/recovery-lower-restored-v2
```

The [cleanup manifest](2026-09-26-hierarchical-lower-outer-cross-cleanup.json)
records removal of replayable raw evaluations after their compact evidence
was retained. Parent final checkpoints were retained for this diagnostic and
subsequently packaged with the exact panels and configs in the
[final evidence archive](2026-09-26-hierarchical-recovery-final-evidence-package.json).
