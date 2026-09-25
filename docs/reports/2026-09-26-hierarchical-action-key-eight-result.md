# Eight-case physical-action Gumbel search probe

Status: closed. Physical-action keying removes the measured case-ID candidate
churn on this bounded paired panel, but it does not improve completion here
and has an integrated runtime cost. It is not promoted to training.

## Controlled question and inputs

Does assigning Gumbel noise by the physical action, rather than its position
after sorting by learned prior, stabilize candidate selection when the same
problem has a different case ID? Use the first eight problems of the retained
`s1` frozen-search source panel, then make a second panel changing only each
case ID. The ordered problems and prefixes are byte-identical. Keep the
generation-87 lower and outer checkpoint bytes, search seed 20260922,
128 simulations per move, six-move horizon, 24 maximum moves, and all other
search controls fixed. Run rank and physical-key search with the original
IDs, a same-ID repeat, and renamed IDs: six full CLI evaluations.

The exact [original panel](../benchmarks/hierarchical-target-causal/action-key-eight-original-panel.json),
[renamed panel](../benchmarks/hierarchical-target-causal/action-key-eight-renamed-panel.json),
[keyed search config](../benchmarks/hierarchical-target-causal/action-key-eight-search.json),
and [matching outer search contract](../benchmarks/hierarchical-target-causal/action-key-eight-outer.json)
are tracked benchmark inputs. The original rank search and outer configs are
`c16-n128-search.json` and the retained generation-87 model config. The
[protocol](2026-09-26-hierarchical-action-key-eight-protocol.json) records
all input, source, binary and runner hashes before the six runs.

| Search and case ID | Completed /8 | Full physical paths matching original-ID run | Wall time | Peak RSS |
| --- | ---: | ---: | ---: | ---: |
| Rank, original | 2 | 8/8 reference | 28.19 s | 1713 MiB |
| Rank, same-ID repeat | 2 | 8/8 | 28.39 s | 1713 MiB |
| Rank, renamed | 4 | 0/8 | 31.31 s | 1705 MiB |
| Physical key, original | 1 | 8/8 reference | 34.77 s | 1705 MiB |
| Physical key, same-ID repeat | 1 | 8/8 | 34.18 s | 1707 MiB |
| Physical key, renamed | 1 | 8/8 | 34.09 s | 1712 MiB |

Every paired first root had the same complete physical action set, with
2,548–2,784 actions depending on the problem. The rank same-ID repeat kept
all 16 initially visited actions and the first selected action in all eight
cases. Renaming IDs under rank retained 8, 8, 8, 9, 8, 8, 8, and 8 of those
16 visited actions and changed the first selection in all eight. Its
completion mask changed from `00110000` to `11100100`: three gains and one
loss on identical problems. The physical-key original, repeat and renamed
runs shared all 16 visited actions, the first selection and the full physical
path in every case, with mask `01000000`. The [summary](2026-09-26-hierarchical-action-key-eight-summary.json)
retains case masks, paired overlaps, full-path agreement, report hashes,
generated-action counts, wall time and per-process peak memory.

## Interpretation and next decision

This confirms the mechanism: the ID namespace changes the rank-indexed
Gumbel assignment and can change both selected actions and apparent
completion. Physical-key assignment removes that particular dependency on
these eight cases. Its one completion is below rank's two under original IDs
and four under renamed IDs. The result is too small and reused from an old
source panel to estimate expected search strength, and it gives no learner
retention evidence.

The physical-key runs average 34.34 seconds; the rank runs average 29.30
seconds. Comparing the two original-ID/repeat runs gives 34.47 versus 28.29
seconds, about 22% more wall time. Peak RSS stays near 1.7 GiB. Keyed search
generated 173,362 macro samples versus rank's 187,116 on original IDs, so
the extra time warrants investigating key computation; different downstream
search work can also affect elapsed time. An ordinal captured before prior
sorting is a narrower, cheaper coupling candidate while the lower guidance
model is frozen. Check action-order correspondence and integrated cost before
using it for on-policy training. The confirmed frozen horizon-12 gain remains
another independent training/search hypothesis.

## Reproduction and provenance

- Source commit `6607de12`, with physical-key implementation committed in
  `bdf06c61`. The release binary SHA-256, original model checkpoint hashes,
  panel hashes and exact six-run matrix are in the protocol. The
  [runner](../../tools/factory-edit-pilot/hierarchical_action_key_eight_run.py)
  is the exact byte copy used on the server; the
  [summarizer](../../tools/factory-edit-pilot/hierarchical_action_key_eight_summary.py)
  applies the physical-command comparison.
- On `ashrimp-arch`, set the documented LibTorch environment, place the
  tracked source assets under `artifacts/gumbel-action-key-eight` as
  `panel-original.json`, `panel-renamed.json`, `search-key.json`, and
  `outer-key/config.json`. Copy the retained outer `state.safetensors` and
  `parameter-order.json` beside that keyed outer config, and copy the
  tracked protocol to `protocol.json` in the experiment root. Run
  `python3 -u tools/factory-edit-pilot/hierarchical_action_key_eight_run.py`,
  then `python3 tools/factory-edit-pilot/hierarchical_action_key_eight_summary.py --output artifacts/gumbel-action-key-eight`.
- This bounded search ran only after the preceding on-policy experiment and
  implementation smoke were closed and their raw outputs removed. It does
  not require W&B training tracking because it does not train a model.
- The [cleanup record](2026-09-26-hierarchical-action-key-eight-cleanup.json)
  removes 179,143,381 bytes of raw reports, logs and copied model/config
  inputs after this compact package was checked.
