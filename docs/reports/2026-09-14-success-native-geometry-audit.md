# Complete-state native geometry audit

> Withdrawn from scientific selection on 2026-09-14 at the user’s request.
> Historical execution evidence only; do not use these results or recommendations
> to select a training setup. Replaced by the [quality-focused study](../designs/2026-09-14-quality-weighted-replay-study.md).

The bounded audit permits the optional geometry package in the frozen-arrival
screen (`geometry_eligible: true`). This is local attribution evidence, not proof
that the heuristic is generally safe or improves a trained policy.

Three actual successes were replayed from complete native checkpoints with their
original request, physical state, port assignments and action order. Each also
received a declared pose reversal and an insertion/pose-contraction prefix.
Native revisions were rebound at each action; complete translated final geometry,
port obligations, validated used area and gain determined comparability.

| Actual segment prefix | Control | Gain | Exterior bonus | Pose reversal | Band/pose |
| --- | --- | ---: | ---: | --- | --- |
| `7d306229af38` | Non-loop compaction | 0.07692308 | 0.05494505 | Decisive, delta 0 | Different endpoint |
| `b7e4114c2c1f` | Outward success | 0.07692308 | 0 | Decisive, delta 0 | Different endpoint |
| `9c7f6ebbcd7c` | Outward success | 0.05208333 | 0 | Decisive, delta 0 | Decisive, delta 0 |

All nine replays were native-valid successes. Four comparisons had equal complete
endpoints and gains; none increased the capped exterior bonus. The two different
endpoints are inconclusive for attribution and do not supply decisive coverage.
Rarity was disabled for this attribution check. Outward success remains admitted.

Historical export IDs `07db6d65`, `49f420e2`, and `28c3cd68` lack complete native
prefixes in this interval and are individually reported as unavailable. Their
compact geometric exports were not substituted for native inputs. Both required
control roles and reversal families have coverage from the new complete corpus.

Preparation inspected four candidates and decoded four source chunks in 2.593 s.
Nine native replays took 0.078 s; combined elapsed time was 2.671 s, below 300 s.
Observed RSS was 781,811,712 bytes. Native limits remained 64 actions, 300,000 ms,
50,000 route expansions and 3,200,000 total route expansions. No budget was raised.

Seven focused tests passed, including actual native revision rebinding, illegal
ports, timeout classification, geometric-export rejection, and false eligibility
when reversal comparisons are inconclusive. Ten shared scorer tests passed after
the bounds helper refactor. Workspace check/tests passed with unchanged Rust.

## Reproduction and provenance

From `/home/ashrimp/Srv/aic-retention-candidates-20260914`, using the existing server
Python environment:

```sh
python tools/factory-edit-pilot/diagnostics/declarative_success_native_cases.py \
  --corpus /home/ashrimp/Srv/aic-retention-experiments-20260914/artifacts/arrival-capture/corpus/manifest.json \
  --output artifacts/native-audit-cases \
  --preferred 07db6d65 --preferred 49f420e2 --preferred 28c3cd68
python tools/factory-edit-pilot/diagnostics/declarative_success_native_audit.py audit \
  --manifest artifacts/native-audit-cases/manifest.json \
  --library /home/ashrimp/Srv/aic-declarative-batch-20260913/target/release/libaic_pilot_native.so \
  --output artifacts/native-audit
```

The manifest SHA-256 is
`80162e468f6bd6dcfd3175c84cdd5eacd2aeeb4d50142603e67a5513dd357eaa`.
Native library SHA-256 is
`dc84066c78d07c60911193e8e32b0d996db4c33ccd9b24378e86fd80cd797226`.
The versioned report binds the script, manifest and all five external game tables.
Raw complete checkpoints/actions/outcomes/final metrics remain under the server
output directory. Compact report and manifest copies are retained locally under
`artifacts/retention-experiments/20260914/native-audit-{report,manifest}.json`.
An initial preparation-only missing-source formatting error was fixed before any
native replay; its partial output supplied no evidence. These commands document
the completed audit and should not be rerun automatically. Main training remains
paused at update 1744.
