# Inert search-horizon probe (stopped)

## Question and method

Does the outer Gumbel MCTS search horizon of 12 explain held-out completion
losses? Compare the retained H12 models against the same models and held-out
cases with horizon 4096. Each root search has 128 simulations and expands at
most one new tree node per simulation, so 4096 cannot truncate that search.
This changes only search behavior; no model is trained. The other controls,
including 24 maximum moves, 16 root candidates, six route candidates and 48
route searches, remain fixed.

The comparison was stopped after the first three paired full evaluations when
inspection found that the constructive session cannot install a catalog bridge
and its route candidate generator always passes zero crossing options. More
search-only repetitions would not exercise this missing required capability.

## Identity and reproduction

- Search binary source commit: `ad28ad04d103b5407de96e83a35ac880d1818e1a`.
- Search binary SHA-256: `66f72a2dbdf45aadbc50e4dfe8f3634012ef5f5a76d8159c698de20f85d7a4fa`.
- Retained input archive: `artifacts/recovery-lower-restored-v2/final-evidence.tar.gz`; SHA-256 `488d68b57343550be13fca36f8bb0456ae32e5876811cdf62e528207f04232b3`.
- Runner: [`hierarchical_uncapped_search.py`](../../tools/factory-edit-pilot/hierarchical_uncapped_search.py); SHA-256 `5c19881653a2e6836fecf2bac665f5218ffb081a6e21f4048c345bdd95e38cdb`.
- Frozen H12 configuration: [`horizon-12-search.json`](../benchmarks/hierarchical-target-causal/horizon-12-search.json). The runner derives exact JSON configurations by changing `horizon` to 4096 and `root_selection.seed` to the evaluation seed, then writes matching outer-model shadow configurations. The first seed's derived config SHA-256 is `31271c20423433479319936b8bbff24f15cdfc5a7b246a7a6a17e9b21e12b5b5`.
- Held-out panel, baseline results, and model state are extracted from that archive. The retained [`protocol manifest`](2026-09-26-inert-search-horizon-protocol.json) SHA-256 is `ead00a74898a09b12e73adb89739867fac21c88fb841ae2b9d3194def0846cf0`; the retained [`three-record progress manifest`](2026-09-26-inert-search-horizon-progress.json) SHA-256 is `70c2c6428ddd9c201ebbac5756bd87a5db989f3e27d169d7b2498784725a73a6`.

Run on `ashrimp-arch` under `~/Srv/aic-target-causal-20260925`, using the
existing release binary and libtorch environment described in
[`training-server.md`](../maintenance/training-server.md):

```sh
python3 tools/factory-edit-pilot/hierarchical_uncapped_search.py prepare \
  --root artifacts/uncapped-search-20260926 \
  --archive artifacts/recovery-lower-restored-v2/final-evidence.tar.gz \
  --binary target/release/aic-cli
python3 tools/factory-edit-pilot/hierarchical_uncapped_search.py run \
  --root artifacts/uncapped-search-20260926 --smoke --max-records 1
python3 tools/factory-edit-pilot/hierarchical_uncapped_search.py run \
  --root artifacts/uncapped-search-20260926 --max-records 3
```

The actual full command included `LIBTORCH`, `LIBTORCH_CXX11_ABI=1`, and
`LD_LIBRARY_PATH` from the server runtime. It was stopped while starting the
fourth evaluation; that partial fourth output was discarded.

## Results and verdict

All full evaluations used seed `20260922` and the 128-case held-out panel.
The progress manifest still says `running` because the process was interrupted
after its third recorded evaluation; this report is its stopped closeout.

| Model stream | H12 completed | Horizon 4096 completed | Paired success mask | H12 seconds | Horizon 4096 seconds | New report SHA-256 |
| --- | ---: | ---: | --- | ---: | ---: | --- |
| s1 | 90/128 | 90/128 | Identical | 354.0 | 357.3 | `90d2c1d606a9a9f65629aacf0552a3cd548afc9f7f2f85d6705b762c9cc89297` |
| s2 | 78/128 | 78/128 | Identical | 368.7 | 364.1 | `13314271f25ef0534e4ea316059d4eb7984097a49055034f674b353419ac38c6` |
| s3 | 75/128 | 75/128 | Identical | 370.7 | 368.9 | `0ab79c56b627d2fd17258cf0dbd4a5422cb2a4bec5a3ed18bf0d44f464de0d9d` |

The 12-step search cap did not change case-level completion status in these
three paired evaluations. This does not establish that the cap is harmless
across other search seeds or larger factories. It also does not test a completed
crossing implementation. The current bottleneck investigation must first close the
known representation gap: `routing_candidates.rs` constructs empty crossings
and sets `crossing_limit: 0`; `constructive.rs` has no bridge-install command and
publishes `bridges: vec![]`. No additional training result should be promoted
as full-layout readiness while that required capability is missing.

## Cleanup

The server experiment occupied 1.4 GiB before cleanup, with 151 GiB free on
the server filesystem. The three successful evaluations, their exact success
masks and report hashes were checked against the archived baseline before raw
reports were removed. The input archive remains the retained source for
reproduction; this stopped experiment has no continuation checkpoint.

The source archive and this report are retained. The experiment's generated
inputs, reports, models, configurations and logs were removed after verification.
The exact paths and byte measurements are in the
[`cleanup manifest`](2026-09-26-inert-search-horizon-cleanup.json).
