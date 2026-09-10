# Recorded facility 15/16 move: a routing witness exists

The failed action is routable. Reproduce action 31 of evaluation batch 368
(cumulative batch 400), episode zero, from the twelve-hour CNN-2 run. It moves
facility 15 from (36,27) to (36,10) and facility 16 from (36,20) to (36,3), with
unchanged rotations. Display labels are one-based placement indices.

Replay the preceding 30 actions and match action, status, current/best witness
hashes, best score and expansion counts. The original action then reproduces its
recorded failure exactly. Freeze its pre-edit geometry and original production
request for isolated probes; change only the routing work allowance.

| Work allowance | Actual expansions | Attempts | Outcome |
| ---: | ---: | ---: | --- |
| 5,000 | 5,000 | 1 | Work exhausted on chain 58; original retained |
| 10,000 | 9,341 | 2 | Complete, independently validated candidate |

At 10,000, attempt zero spends 25 + 216 + 8,531 = 8,772 expansions and establishes
`no-path` for chain 58 under that attempt's boundaries, directions and port choice.
The existing retry then changes facility 16's input from `input-belt-0` to
`input-belt-1`, prioritizes the failed chain, and completes four routes with
113 + 211 + 192 + 53 = 569 expansions. The four-chain repair set stays fixed;
no extra chains or relaxed factory rules are needed.

This is evidence of a poor first port/order choice consuming the shared work
allowance before a useful retry, not evidence of global routing infeasibility.
Port choice and order both change in attempt one; their individual causal effects
have not been isolated. End-to-end isolated edit times were about 43 and 52 ms
on the training host during concurrent training; these are not stable latency
benchmarks.

The valid candidate does **not** improve area: score changes from
`[1394, 484, 108]` to `[1394, 563, 122]` (area, transport tiles, turns). It is an
accepted exploration state, while the preceding best remains better. This probe
proves that the exact requested movement can be embedded, not that increasing
all training budgets would improve learning or final layout quality.

## Reproduction

Run from the frozen runtime `/home/ashrimp/Srv/aic-ppo-cnn2-12h-20260910` with its
existing native binaries, catalogs and Python environment:

```sh
PY=tools/factory-edit-pilot/.venv/bin/python
$PY tools/factory-edit-pilot/probe_ppo_recorded_action.py \
  --run artifacts/factory-edit-pilot/cnn2-twelve-hour \
  --job current-history-16--seed-17 --phase evaluation --batch 368 \
  --episode-index 0 --action-number 31 --work-budgets 5000 10000 \
  --output artifacts/factory-edit-pilot/pose-15-16-step31-verified
```

Use a fresh output directory. The CLI verifies frozen runtime inputs, replays
recorded actions, matches the isolated baseline's physical execution, preserves
its exact input and action, and independently admits any found witness against
the original production request. It stops at the first validated candidate.
Fresh-episode best/reward accounting is not compared to historical best accounting.
The ongoing training protocol, model and budgets are unchanged.

Retain raw replay, region attempts and witnesses outside Git in the output path
above. The adjacent provenance manifest binds the minimal identities. CLI probes
passed; `cargo check --workspace` and `cargo test --workspace` passed. No Rust
behavior changed.
