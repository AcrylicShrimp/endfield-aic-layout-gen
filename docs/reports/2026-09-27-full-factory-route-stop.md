# Full-factory route reembedding stop closeout

## Question and method

Could a finite routing or region-retry limit make full-factory reembedding
appear geometrically impossible, and were the default limits adequate at the
supported factory scale? We compared the previous implementation
(`98ee9cdef7825118630b9709944c3248772bba86`) with the reviewed change
(`d5bde659db8b7de19d466182a8560b5d563fc33d`) on fixed selected actions.
The baseline applied only
`docs/benchmarks/full-factory-route-stop/baseline-panel.patch.txt` to add the
ignored measurement tests. The final release binary's five changed source-file
SHA-256 values matched the committed files, as did the measurement script.
No training or CUDA workload was launched.

The real control was the retained 59-facility witness at
`artifacts/factory-edit-pilot/full-factory-pose-historical-input/witness.json`
(SHA-256 `15c3ec1375d97532ac7ddcf2bc57fb28164561a83458c267082960c7df1e6530`).
The upper-scale capacity panels used 51 copies of the tracked five-facility
crossing fixture, spaced by 30, 60, and 120 cells. Each had 255 facilities and
765 chains. These are three density arrangements of **one repeated topology**;
they do not establish the distribution seen in training. Plan .08 owns that
distribution check. Each witness passed `FactoryEditSession::new` admission.

We measured fixed pose, port, reroute, component and bridge-release action
samples with 50,000 shared route expansions and 25,000 per attempt, plus a
small route panel, forced route stops, explicit chain caps, and larger-budget
controls. The server was `ashrimp-arch` CPU; all temporary sources, inputs,
builds and logs stayed under `~/Srv/plan19-route-20260927/`.

## Result

The route result now records one `PathResult`, including scans and peak
frontier entries. Only complete local no-path/invalid-endpoint outcomes are
local rejection. Work, scan, frontier, deadline, allocation, numeric and
repeated-cell stops preserve `search-incomplete` and their charged work.
Region retries retain the first unfinished reason through later failures;
the final no-witness status, report termination and diagnostic agree. The
original witness remains intact until a complete candidate is accepted. The
region report contract is `factory-route-region-v3`. The outer generation
record includes a compact route and region stop summary only for an unfinished
search. The hidden 200,000 `execute` work-input ceiling and one-million
per-attempt input ceiling are gone.

| Case | Source | Selected actions | Candidate | Rejected | Incomplete | Unchanged | Expansions | Panel wall | Process peak RSS |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Historical 59 | old | 87 | 23 | 48 | unreported | 16 | 289,185 | 251 ms | 18,140 KiB |
| Historical 59 | new | 87 | 24 | 37 | 10 | 16 | 448,713 | 352 ms | 17,972 KiB |
| Compact 255 | old | 96 | 58 | 27 | unreported | 11 | 1,320,014 | 1,269 ms | 44,888 KiB |
| Compact 255 | new | 96 | 58 | 2 | 25 | 11 | 1,320,014 | 1,306 ms | 44,984 KiB |
| Balanced 255 | old | 96 | 60 | 25 | unreported | 11 | 1,267,945 | 1,333 ms | 46,864 KiB |
| Balanced 255 | new | 96 | 60 | 2 | 23 | 11 | 1,267,945 | 1,307 ms | 49,032 KiB |
| Spacious 255 | old | 96 | 65 | 20 | unreported | 11 | 1,158,657 | 1,237 ms | 60,180 KiB |
| Spacious 255 | new | 96 | 65 | 1 | 19 | 11 | 1,158,657 | 1,282 ms | 60,556 KiB |

The old implementation placed work-stopped actions inside `rejected`.
Therefore, the changed rejection/unknown split is a correction of the verdict,
not a loss of candidates. The historical 59-facility panel gained one complete
candidate with eight default attempts instead of four. All 255-facility
candidate counts stayed unchanged. Panel times include action execution after
enumeration; the process RSS includes the test harness and witness. Individual
wall figures are observations, not a guaranteed latency bound.

The sampled initial affected region needed at most ten chains in the historical
factory and eight in the replica. A full scan of 2,952 historical and 35,339
replica actions found no affected region above 64 chains. But a legal direct
129-chain edit stopped at an explicit 64 or 128 cap with
`region-chain-limit`, zero expansions, and the actual count. With cap 256 it
reached the one-expansion work stop; all 765 chains were admitted with the new
default and also stopped as unknown after one expansion (4.3 ms on the spacious
replica). Thus the old cap could censor legal requests even though these
sampled actions did not hit it. The new default admits all chains in the
witness; callers may still request a smaller explicit cap.

On a fixed route panel, found-path steps and candidate witness hashes were
unchanged for the five-facility fixture, historical factory and spacious
replica. A five-facility crossing pose still produced two bridge crossings,
1,013 expansions and the same candidate hash
`5b13dfe93a2ba3e15ed958a0d4edbbfe7cad7f4c3162ebb98360e5b43f380d9b`.
A forced one-expansion reroute changed from `rejected` to
`search-incomplete`, retaining the `work-budget` path status. A focused retry
case verified both a later successful candidate after an unfinished attempt
and a final unknown after later complete local failures. The outer randomize
case retained the route stop summary without accepting a partial witness.

## Limit calibration and residual uncertainty

The default remains 50,000 shared work and 25,000 per attempt in the measured
caller, with eight attempts, four alternative ports, eight extra chains, and
eight crossings per route. The historical 59-facility panel's ten unfinished
actions were three attempt, two extra-chain and five port-trial stops. Raising
attempts to 32, port trials to 16 and extra chains to 32 resolved eight more
verdicts as complete local failures but found no further candidate. A
fourfold work allowance on the compact replica found only one more candidate
(58 to 59), approximately doubled route execution time, and raised peak RSS
from about 45 to 67 MiB. The balanced and spacious replicas gained none.
At one million shared work, the compact replica still produced 59 candidates.
There is no evidence here for an unconditional 4x default work increase.

The original retry loop revisited the same edit, port assignments and route
priority many times: one 32-attempt historical action had only three distinct
retry states. We skip a repeat only when the edit state **and its effective
per-attempt work budget** are identical. The router's behavior need not be
monotone in budget, so a lower-budget retry remains eligible. A fresh reviewer
identified the missing budget dimension in the first implementation; it was
added, the panels were rerun, and their final candidate and status counts were
unchanged. The release panel completed with peak RSS below 61 MiB at default
work in all four measured cases.

These panels establish finite-stop classification and supported-size capacity,
not general search adequacy. Plan .08 must measure the actual generated factory
topologies and scale distribution before long training. Plans for the remaining
candidate and learner modules still gate follow-on experiments.

## Reproduction and verification

```sh
python3 docs/benchmarks/full-factory-pose-scale/generate-stress.py --kind replicated --spacing 30 --output target/route-compact30.json
python3 docs/benchmarks/full-factory-pose-scale/generate-stress.py --kind replicated --spacing 60 --output target/route-balanced60.json
python3 docs/benchmarks/full-factory-pose-scale/generate-stress.py --kind replicated --spacing 120 --output target/route-spacious120.json
cargo test --release -p aic-pilot-native --lib --no-run
python3 docs/benchmarks/full-factory-route-stop/measure.py --source . --witness target/route-compact30.json --output target/route-compact30-result.json --test measure_region_default_panel

git worktree add --detach target/route-baseline 98ee9cde
git -C target/route-baseline apply "$PWD/docs/benchmarks/full-factory-route-stop/baseline-panel.patch.txt"
(cd target/route-baseline && cargo test --release -p aic-pilot-native --lib --no-run)
python3 docs/benchmarks/full-factory-route-stop/measure.py --source target/route-baseline --witness target/route-compact30.json --output target/route-old-compact30.json --test measure_region_default_panel
```

The ignored `measure_region_route_panel`, `measure_region_action_chain_counts`
and `measure_region_chain_capacity` tests use the same script with `--test`.
The historical witness path above can replace the stress witness. `--work`,
`--attempt-work`, `--attempts`, `--port-trials` and `--extra-chains` specify
the calibration controls. No standalone CLI exposes this internal route stage;
the full-factory generation test exercises the outer action record directly.

`cargo fmt --all`, `cargo check --workspace`, `cargo test --workspace`, the
tracked-file worktree and index checks, and the focused release panels passed.
The independent implementation review returned `PASS_WITH_NOTES` after the
retry-key correction. Its remaining note is the synthetic nature of the
255-facility scale panels.

## Cleanup manifest

After verifying the committed implementation, tracked baseline patch and
measurement script, historical witness, stress generator, and this readable
closeout, the regenerable server directory
`~/Srv/plan19-route-20260927/` was removed and confirmed absent. It contained
baseline/current source exports, release build targets, generated stress
witnesses, raw panels and temporary logs; no continuation checkpoint was
needed. Its measured size before removal was 890,583,492 bytes. Server free
space changed from 162,017,865,728 to 162,933,178,368 bytes. The historical
59-facility witness remains at the local artifact path above; the replica
inputs can be reconstructed from the tracked fixture and generator.
