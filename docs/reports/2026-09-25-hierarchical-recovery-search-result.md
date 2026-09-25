# Hierarchical recovery search screen

Status: the 54-cell frozen-model search screen and mechanism analysis are
complete. Only horizon 12 passed the predeclared screen gate. This is a
training-source screen; a new, disjoint panel must confirm it before the search
contract changes.

## Question and method

Test whether the current root allocator gives short-horizon Q too much weight,
whether its anchor candidate quota misses useful actions, or whether the
six-move planning horizon truncates consequences. The same 48 frozen
generation-87 cases, model bytes, 16 candidates, 128 simulations per move,
24-move maximum, and three root seeds were used for six search arms. Only the
named control changed. `q02` and `q001` set Q scale to 0.02 and 0.001 instead
of 0.1; `anchor16` and `anchor0` guaranteed 16 and zero anchor candidates
instead of eight; `horizon12` doubled horizon six to 12. The CLI rejected the
initial exact-zero proposal before search; its separate
[closeout](2026-09-25-hierarchical-recovery-search-invalid-zero.md) records the
validated replacement and migrated cells.

| Arm | Completed by seed, /48 | Three-seed mean | Cases successful under ≥2 seeds | Baseline robust successes lost | Integrated wall time |
| --- | --- | ---: | ---: | ---: | ---: |
| Base | 19, 16, 14 | 16.33 | 11 | 0 | 500.7 s |
| Q scale 0.02 | 13, 15, 10 | 12.67 | 9 | 3 | 520.2 s |
| Q scale 0.001 | 8, 10, 4 | 7.33 | 4 | 7 | 514.7 s |
| 16 anchors | 15, 20, 22 | 19.00 | 20 | 5 | 511.2 s |
| Zero anchors | 19, 16, 14 | 16.33 | 11 | 0 | 518.4 s |
| **Horizon 12** | **26, 29, 29** | **28.00** | **28** | **0** | **546.8 s** |

Horizon 12 gained 35 validated completions over 144 paired case-seed trials,
an average of 11.67 additional completed cases per 48-case seed. It improved
all three seeds, preserved all 11 baseline cases that succeeded under at least
two seeds, and increased that robust count to 28. Its wall time was 9.2%
higher than baseline. It passes the declared gate of mean gain at least three,
gains in at least two seeds, and at most two lost baseline robust successes.
The other arms fail it. Sixteen anchors improved the mean but lost five robust
baseline successes; zero anchors changed the considered sets but selected the
same first action in every 48-case seed and completed exactly the same cases.
These are paired observations, not independent success trials.

## What the longer horizon changed

The search's unfinished-state progress value and sigmoid model completion
value are at most one. A visited root action with Q above one therefore
witnesses a completed-layout leaf in at least one simulation. Among the first
four roots per episode across all cases and seeds, this happened in 49/576
baseline roots and 190/576 horizon-12 roots. The larger horizon exposed more
actual finishes to the action ranking. The first action still matched baseline
in 33, 26 and 26 of 48 cases by seed; many gains came from later decisions.
This supports a specific mechanism but does not prove every gained case was
caused by an observed terminal leaf.

The policy-learning signal remains weak at the widest roots. Horizon 12
reduced mean first-root target mass on unvisited actions only from 99.47% to
99.09%, and its effective action fraction was still 98.23% of the full action
set versus 99.38% for baseline. The mean later-root unvisited mass declined
from 65.07% to 62.43%. On the previously independently confirmed first-action
pairs, original-seed searched Q pointed to the winner/loser/tie in 3/10/1
baseline pairs and 4/9/1 horizon-12 pairs. Their case-mean pair target losses
were 1.886 and 1.883 nats. Only one confirmed pair was doubly visited under
each of the other two seeds, so those seed comparisons cannot establish first
root value quality. A nearly prior-only Q scale had a lower pair loss but far
worse completed-layout search; this selected pair set is not a sufficient
search-selection metric.

The screen shows a real improvement in frozen search reliability, not yet a
learner repair. It cannot estimate transfer beyond these 48 previously used
two-facility training cases. The next stage compares base and horizon 12 on
64 new disjoint problems under three seeds using the separately declared
confirmation gate. If confirmed, a learner must still be trained and evaluated
with fresh on-policy collections before changing the curriculum.

## Provenance and reproduction

The [protocol](2026-09-25-hierarchical-recovery-search-protocol.json) records
the 48 panel hashes, checkpoint/configuration identities, source commit
`898eb3d1`, and release binary SHA-256
`e594b5425074a03731524d8c91991126232d840ef44a86e8b0630e1f16800fdf`.
The [progress](2026-09-25-hierarchical-recovery-search-progress.json) hashes
all 54 raw reports and records wall times. The
[compact result](2026-09-25-hierarchical-recovery-search-compact.json) retains
each case's three-seed success and first-action-agreement masks, the full
search-summary and action-alignment analysis hashes, and the mechanism and
target metrics. The search code identity is the protocol commit; the
post-screen action-alignment and compaction scripts are commits `ce918274`
and `3f4d9cf1`. No search or training source patch was uncommitted.
W&B was not required for this frozen inference screen.

With the documented libtorch environment exported under `~/Srv`:

```bash
cd ~/Srv/aic-target-causal-20260925
bash tools/factory-edit-pilot/prepare_hierarchical_target_quality.sh \
  target/release/aic-cli artifacts/recovery-search-v2
python3 tools/factory-edit-pilot/hierarchical_recovery_search.py prepare \
  --output artifacts/recovery-search-v2 --experiment-commit 898eb3d1
python3 tools/factory-edit-pilot/hierarchical_recovery_search.py run \
  --output artifacts/recovery-search-v2
python3 tools/factory-edit-pilot/hierarchical_recovery_search.py summarize \
  --output artifacts/recovery-search-v2
python3 tools/factory-edit-pilot/hierarchical_recovery_action_alignment.py \
  --output artifacts/recovery-search-v2
python3 tools/factory-edit-pilot/hierarchical_recovery_compact.py \
  --output artifacts/recovery-search-v2
```

The cleanup manifest records removal of the replayable search reports after
the compact package was verified. Three base-seed source collections and the
model/configuration inputs remain temporarily required by the declared
successful-action learner screen and independent horizon confirmation.
