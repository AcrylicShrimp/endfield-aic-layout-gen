# Fresh-panel horizon confirmation, with a namespace deviation

Status: closed as a numerical confirmation with a protocol deviation. The
predeclared namespace must be checked separately before promoting horizon 12.

## Question and method

Does the frozen horizon-12 search improvement survive a fresh 64-case panel?
The generation-87 lower and outer checkpoints, 128 simulations, 16 root
candidates, other search controls, and three root seeds were held fixed. The
panel was generated before results, excluded all 48 screen problem identities,
and used seed `20262560`. Each seed compared horizon 6 and 12 on the same cases.
These are training-partition cases, not the final held-out learner evaluation.

| Search | Completed layouts, seeds 20260922/23/24 | Success in at least 2 seeds | Total wall time |
| --- | --- | ---: | ---: |
| Horizon 6 | 25 / 21 / 32 of 64 | 25 | 604.1 s |
| Horizon 12 | 35 / 37 / 38 of 64 | 37 | 638.6 s |

The paired completion deltas were +10, +16, and +6, with gains/losses of
17/7, 17/1, and 12/6. Three of the baseline's 25 robust successes were lost;
15 new robust successes were gained. The first action matched the baseline in
37, 33, and 36 cases, respectively. Horizon 12 cost 5.7% more wall time in
these six runs. The fixed numerical gate (mean gain at least 4, at least two
seed wins, no more than three lost robust successes) passed, with the robust
loss exactly at its allowed limit.

## Protocol deviation and interpretation

The research design specified namespace
`hierarchical-recovery-horizon-confirm-v1`. The reused panel helper actually
set `hierarchical-target-learner-confirmation-v1`. This was fixed before any
result was observed and did not select favorable cases, but it violates the
declared sampling protocol. In the generator, namespace changes the case ID
only: problem sampling uses the numeric seed, and the compiled layout problem
does not contain the namespace. The search does hash case ID into its Gumbel
root noise, however, so changing the namespace can change selected moves and
completion counts even on the same graphs. The observed gain is evidence for
this case-ID assignment, not yet a strict confirmation of the declared run.

The next bounded check should verify graph equality under both namespaces and
compare their paired search results under the same three seeds. Promotion
requires the declared namespace and the original fixed gate. This remains a
frozen search comparison, not evidence that training retains the gain.

## Reproduction and provenance

- Local experiment source commit: `f551c2fc`; runner:
  `tools/factory-edit-pilot/hierarchical_horizon_confirmation.py`.
- Remote workspace: `~/Srv/aic-target-causal-20260925`; output:
  `artifacts/recovery-horizon-confirm`.
- From that remote workspace, with its documented LibTorch environment:
  `python3 tools/factory-edit-pilot/hierarchical_horizon_confirmation.py prepare --output artifacts/recovery-horizon-confirm --experiment-commit f551c2fc`,
  then `run` and `summarize` with the same `--output`.
- Search binary SHA-256:
  `e594b5425074a03731524d8c91991126232d840ef44a86e8b0630e1f16800fdf`.
- [Protocol](2026-09-25-hierarchical-horizon-confirmation-protocol.json) records
  the input/config/checkpoint hashes; [progress](2026-09-25-hierarchical-horizon-confirmation-progress.json)
  records each raw report hash, byte size, and wall time; [summary](2026-09-25-hierarchical-horizon-confirmation-summary.json)
  retains paired case results and the fixed gate.
