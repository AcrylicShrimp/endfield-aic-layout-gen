# Hierarchical policy stability: independent-stream confirmation

## Verdict

**Neither successful-root replay treatment passed the predeclared confirmation
gate.** The promising one-stream replay effect did not reproduce on three new
paired training-panel streams. B (replay with failed-root policy targets) gained
only 1.33 completed cases of 64 on the three-stream average final-two mean
relative to A, below the four-case gate. D (replay without failed-root policy
targets) lost 3.83 cases relative to C. Both replay arms lost more than one
third of their generation-5 solved cases by generation 7 in every stream.
The next research question is update magnitude and case churn, as specified in
the [confirmation design](../designs/2026-09-23-hierarchical-policy-stability-confirmation.md).
No treatment advances to lower-training integration or an ultra-long run.

This conclusion concerns fixed two-facility cases with the lower learner
frozen. It does not establish that replay cannot help under a different update
budget or that failed-root policy targets should be removed in production.
The former 128-case validation panel and any final held-out panel were not read.

## Controlled comparison

All 12 arms started from the same generation-87 lower and outer weights and
used eight generations, 32 training cases per generation, eager Gumbel search,
200 epochs, 256 outer root-state slots and 3,200 outer updates per generation.
Within each new stream, A/B/C/D used byte-identical training panels. The new
64-case diagnostic panel was disjoint by problem identity from all 24 training
panels and the previous screen's diagnostic panel. The common start completed
20/64 on it, including 7/14 cases with at most four connections. Evaluations
were fixed at generations 1, 3, 5 and 7.

| Stream | A evaluations | B evaluations | C evaluations | D evaluations | B−A final-two mean | D−C final-two mean |
| --- | --- | --- | --- | --- | ---: | ---: |
| S1 | 29, 22, 25, 21 | 20, 21, 19, 27 | 22, 25, 25, 33 | 20, 27, 24, 18 | 0 | −8 |
| S2 | 24, 20, 22, 30 | 23, 25, 31, 25 | 18, 15, 22, 28 | 21, 26, 19, 24 | +2 | −3.5 |
| S3 | 22, 23, 20, 24 | 25, 27, 28, 20 | 17, 28, 29, 20 | 26, 20, 25, 24 | +2 | 0 |

Counts are completed cases of 64 in generation order 1, 3, 5, 7. The first
screen's D trajectory had risen at every evaluation. None of the three new D
trajectories is monotone. Its earlier pattern was therefore specific to that
training flow; case-level churn was already present in the first screen.

| Replay contrast | Three-stream mean gain | Three-stream minimum gain | Streams with positive mean gain | Streams passing easy and retention guard | Required |
| --- | ---: | ---: | ---: | ---: | --- |
| B over A | +1.33 | +0.33 | 2/3 | 0/3 | ≥4, ≥3, ≥2/3, ≥2/3 |
| D over C | −3.83 | −2.00 | 0/3 | 0/3 | ≥4, ≥3, ≥2/3, ≥2/3 |

B lost 10/19, 18/31 and 19/28 of its generation-5 solved cases by generation
7 in S1, S2 and S3. D lost 15/24, 12/19 and 15/25. All exceed the one-third
limit. B's final easy-stratum counts were 11, 9 and 6 of 14, versus 7 at the
common start; D's were 6, 9 and 5. This is failure of the stability mechanism,
not merely a final-score tie. C has the highest descriptive pooled final-two
mean (26.17/64), but no no-replay arm had a predeclared promotion gate, and C's
trajectory still churns. It is not selected.

## Integrity, runtime and limits

All 96 generations completed with the expected per-stream panel hashes,
3,200 updates, 256 outer slots, unchanged lower checkpoint and declared
failed-policy/replay controls. All 12 W&B runs have acknowledged history
through generation 7. S2-B's initial observer missed remote step 7 after
training completed; its committed generation-7 metrics were backfilled to the
same run and the observer resumed. S3-D's observer succeeded on the runner's
single retry. The [tracking recovery record](2026-09-23-hierarchical-policy-stability-confirmation-tracking-recovery.md)
describes S2-B and its duplicate remote history rows. Local atomic reports
and their hashes are authoritative.

The 12 integrated arm times ranged from 27.1 to 28.5 minutes, 332.2 minutes
in total. The release binary SHA-256 was
`e594b5425074a03731524d8c91991126232d840ef44a86e8b0630e1f16800fdf`,
the same as the first screen. The training source is commit `31555f5`; the
predeclared protocol and configurations are commit
`f23f3b3c0899c05164231e1e8243b229d2c4d5d2`, and the recovery runner is
commit `188512479cfdb9ab5dae371b940f4c8ca235d587`. There was no uncommitted
training patch. The [launch manifest](2026-09-23-hierarchical-policy-stability-confirmation-launch.json)
contains all prepared panel, model, configuration and binary hashes.

The compact [S1](2026-09-23-hierarchical-policy-stability-confirmation-s1-evidence.json),
[S2](2026-09-23-hierarchical-policy-stability-confirmation-s2-evidence.json) and
[S3](2026-09-23-hierarchical-policy-stability-confirmation-s3-evidence.json)
evidence contains per-case four-evaluation success masks, gains/losses,
connection strata, root diagnostics, model movement, replay and policy-root
counts, integrated times, output hashes and W&B links. Bit 0 through bit 3 of
each hexadecimal case mask correspond to generations 1, 3, 5 and 7 in the
listed diagnostic-case order. The [summary](2026-09-23-hierarchical-policy-stability-confirmation-summary.json)
applies the fixed gates. The [baseline evidence](2026-09-23-hierarchical-policy-stability-confirmation-baseline-evidence.json)
records the common-start case outcomes.

From `~/Srv/aic-policy-stability-20260923`, before raw cleanup, reproduce the
compact analysis with the retained PyTorch environment:

```bash
python="$HOME/Srv/aic-rust-learning-20260915/.venv/bin/python"
for stream in s1 s2 s3; do
  "$python" tools/factory-edit-pilot/analyze_hierarchical_stability_confirmation.py \
    --confirmation artifacts/policy-stability-confirmation \
    --inputs artifacts/policy-confirmation-inputs --stream "$stream" \
    --output "artifacts/policy-stability-confirmation/analysis/$stream.json"
done
"$python" tools/factory-edit-pilot/summarize_hierarchical_stability_confirmation.py \
  --evidence-dir artifacts/policy-stability-confirmation/analysis \
  --output artifacts/policy-stability-confirmation/analysis/summary.json
```

The original launch command is in
`tools/factory-edit-pilot/run_hierarchical_stability_confirmation.sh` and the
same script's `--resume` mode continued after S2-B's telemetry repair. The
source panel seeds and namespaces are in the design and tracked curriculum
configurations. After verifying that the committed evidence matched the server
analysis byte for byte, the experiment's raw outputs, generated inputs and
build cache were removed. The [cleanup manifest](2026-09-24-hierarchical-policy-stability-confirmation-cleanup.json)
records 12,122,023,743 file bytes removed and filesystem free space rising
from 166,808,158,208 to 178,862,133,248 bytes. The separate canonical
generation-87 checkpoint remains available for the next bounded investigation.
