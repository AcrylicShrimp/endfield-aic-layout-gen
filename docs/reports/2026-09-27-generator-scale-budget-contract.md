# Upper-scale procedural generator budget contract

## Question and method

Plan [22](../plans/2026-09-27.22-generator-scale-budget-contract.md) asks whether the procedural generator's own size, route-work, pair-trial, workspace and JSON guards prevent legal 256-facility layouts. This is a generator admission and output test, not a claim that its distribution is ready for training. The original generator was run at fixed seeds 43000–43003, first with only its invalid configuration ceiling lifted to expose the old 8,192-pair and 250-million-work stops. Intermediate policies increased pair and work limits separately. The final `scale-policy-v4` uses the checked size-dependent work formula and pair factors 32/192/288 for Fast/Standard/Thorough.

Source configurations are tracked in [layout-generator-scale/configs](../benchmarks/layout-generator-scale/configs/) and described by its [README](../benchmarks/layout-generator-scale/README.md). The comparison is one raw construction attempt per survey slot. Three-seed surveys use three workers and a shared wall deadline, so their elapsed time is a batch wall time, not a per-case latency. `Generate` was also run to persist complete layouts, followed by independent `Validate` on the same catalog.

The base checkout was commit `1ca62ea03aca2d0adda9670ab2a1297f6f51d696`. The old-policy probe overlay removed only the inconsistent one-million per-call config rejection so an upper-scale request could reach the old search limits. Its source archive SHA-256 is `11dd11a90785f39be7308b0696dad103e4c6c8d62eafc1910908541d52844a0f`. The v2 overlay used Standard/Thorough pair factors 64/96 and additional 1/2-billion per-attempt work ceilings (archive `8ff1144eb3f8513df6b80538b7039782fd1c76b193919cd79f0dbe671dec2991`). The separate pair probe replaced the v2 pair allowance with 65,536 and left its work ceiling unchanged (archive `dfad6b8c725b8e1f78a1631f776ac2709fd149dec62a6e5fd5fdd9ffc5ba78ac`; exact probe `intent.rs` SHA-256 `c8e60544a8e63ee82205580368156fd3818d29f28eecb19c6fffe9e4c06e2ef1`). The v3 full-work probe removed the v2 1/2-billion ceiling while retaining its pair factors; its release binary SHA-256 is `83e89df0d470c6cf8bd42423747d355643ab6e7a825ae83ebd0e2e22e0a0a1cb`. Its result is compared with v4 below; v4 is the only adopted policy. The server v4 survey binary SHA-256 is `e0366477aee6d6ec7c8b5f89315033e4872807636f5eb0c4be5301fda0721bb0`, with its crate-source archive SHA-256 `4333cfd11047d7117ab934b00847829bdf74413053897aacf185d69e176defad`. This snapshot predates only the small-DAG diagnostic added after the reviewer found an impossible two/three-facility configuration; no 256-facility measurement path changed. The final committed generator code is `2501036c802fc7000b87dae06e3c2a2fd0138a31`.

The uncommitted crate-source overlays are tracked as exact patches in [generator-scale-overlays](generator-scale-overlays/). The old-policy, v2, pair-probe and server v4 patches were applied to the stated base commit in isolated directories and byte-compared with all 12 source files in their retained source archives. The v3 source was overwritten before archival; a reconstruction from v4 did not reproduce the retained v3 binary SHA, so that intermediate three-seed result is exploratory only. Its exact binary is retained locally while the source provenance remains unresolved. The exact v4 same-seed repeat below supplies the reproducible evidence for the final verdict. The 131,072-pair crossing probe applies the [tracked two-file diff](generator-scale-overlays/crossing-pair131072-from-pair.patch) to the 65,536-pair probe source. The tracked patches make the other code evidence reconstructible without the ignored local archives.

## Resource-stop evidence

The original checkout rejects 256 Tree Spacious Standard before generation: its 2,621,440 per-call work request exceeds its own one-million validator ceiling. After lifting only that invalid validator ceiling, seed 43000 gives:

| Request | Connected / target | Last stop | Route expansions | Wall seconds | Child RSS KiB |
| --- | ---: | --- | ---: | ---: | ---: |
| 256 Tree Compact | 227 / 255 | pair trials at 8,192 | 168,417,500 | 120.3 | 122,512 |
| 256 Tree Balanced | 252 / 255 | work at 250,000,000 | 250,000,000 | 124.7 | 210,668 |
| 256 Tree Spacious | 139 / 255 | work at 250,000,000 | 250,000,000 | 117.7 | 626,480 |
| 160 Branching Compact | 158 / 239 | pair trials at 8,192 | 65,900,614 | 50.2 | 102,096 |
| 160 Branching Spacious | 181 / 239 | work at 250,000,000 | 250,000,000 | 112.9 | 342,236 |

The v2 policy completed the same 256 Tree Compact, Balanced and Spacious seed at 187,180,892, 263,858,781 and 713,023,043 expansions, and the 160 Branching Spacious seed at 385,004,090. Their wall times were 156.6, 135.0, 339.8 and 177.0 seconds, with 122,528, 210,524, 626,408 and 346,416 KiB child RSS respectively. Concurrent server load differed between runs, so wall time is descriptive rather than a controlled speedup ratio. This proves at least four old stops were censoring successful searches. The complete 256 outputs had 29,058, 49,946 and 117,286 cells respectively; their serialized JSON files were 478,446, 665,062 and 945,564 bytes, all far below the shared 16-MiB producer/reader allowance. Their respective routed patterns contained 158/97, 162/93 and 169/86 belt/pipe connections, and 578, 736 and 767 bridges. Each persisted output passed independent physical validation.

The predetermined 256-facility v2 seeds 43001–43003 then showed where the intermediate caps still mattered:

| Topology / packing | Successful raw slots | Stop evidence |
| --- | ---: | --- |
| Tree Compact | 1 / 3 | two pair-exhausted at 16,320 trials |
| Tree Balanced | 3 / 3 | 295–436 million expansions |
| Tree Spacious | 3 / 3 | 880–994 million expansions; one success used 99.37% of the v2 one-billion allowance |
| Branching Compact | 0 / 3 | all pair-exhausted at 24,512 trials |
| Branching Balanced | 3 / 3 | 504–729 million expansions |
| Branching Spacious | 0 / 3 | all stopped exactly at one billion expansions |

With the final v4 full checked work formula and Standard pair factor, the same three Branching Spacious seeds completed 383/383 connections at 1,567,003,531, 1,313,299,530 and 1,799,366,115 expansions. The exact-source three-worker survey took 894.08 seconds and reached 1,102,488 KiB child RSS. Their layouts occupied 124,254, 113,988 and 132,990 cells. These final-policy results independently repeat the exploratory v3 outcome and show that the prior one-billion ceiling was too low for all three measured requests.

At 65,536 pair trials, Branching Compact seed 43003 completed 383/383 connections after 36,943 pair trials, 216,310,775 expansions, and a 394-second one-worker `Generate` call. Its complete JSON was independently validated. Seed 43001 completed 383/383 after 367,693,548 expansions when its wall deadline was raised from 600 to 1,200 seconds; the earlier 600-second `interrupted` row was an unknown, not a failed attempt. Seed 43002 remained pair-exhausted at 254/383 despite the larger allowance, so its attainable target is unresolved. The final Standard 73,536-pair allowance exceeds the measured 36,943-trial success with margin; final-policy same-seed results are recorded below.

## Admission, publication and continuity

`max_cells` now follows the checked `max(65,536, 16*n*n)` intent formula, reaching 1,048,576 at 256 facilities. The low-level placement maximum is 250,000,000 cells; an offline sparse placement and mechanism packing both accepted legal canvases above the removed 250,000-cell ceiling without allocating a dense grid. Public Compact/Balanced/Spacious 256-facility layouts in this sweep had smaller actual physical areas, so the public profile gaps were not inflated just to cross that old threshold.

The config validator admits the full 256-facility Thorough Branching Spacious intent: 5,242,880 work per call, 4,016,046,080 per attempt and 110,304 pair trials, below its portable `u32::MAX` work bound and 131,072-pair bound. Three semantically impossible small-DAG requests are rejected before sampling with the target and directed-pair maximum, rather than reducing their requested topology. Checked workspace estimate arithmetic and an explicit `--workspace-budget-gib` setting preserve the two-GiB default. A 256 Tree Spacious Thorough request was rejected under the default with an estimated 3,623,878,656-byte need, and admitted with eight GiB; the report records the chosen budget.

The producer and reader use one 16-MiB JSON allowance. Complete layout JSON is written to a temporary path and renamed only after a successful flush. The original 100-facility Tree Spacious control and the repaired generator produced byte-identical 298,750-byte layout JSON at seed 43000 (SHA-256 `094f2beec0d925a1fda86e6930685138e8a2c8d7cd9d10e0d86b6374784902a4`). This confirms the repair did not change that unaffected layout.

The final v4 `Generate` also persisted 256 Branching Spacious seed 43001 with 383/383 connections after 1,567,003,531 expansions and 6,023 pair trials. Its layout had 124,254 cells, 274 belt and 109 pipe connections, 1,245 bridges and 0.406 occupancy. The 1,333,196-byte JSON SHA-256 is `5b4879631e7b78059da0c10c7c817a1f75ca3363a83e17f72f2a51f42ba798b3`. Independent `Validate` read it through the bounded loader, reported `physical_validated=true` and reproduced the metrics; no temporary file remained. The `Generate` call took 767.46 seconds. The file-size allowance is more than twelve times this largest observed output, so raising it would add no currently demonstrated capability.

## Final-policy confirmation

The final v4 Standard policy repeated the predetermined three Branching Compact seeds under a 1,200-second common deadline. Seeds 43001 and 43003 completed 383/383 connections at 367,693,548 and 216,310,775 expansions; seed 43002 remained pair-exhausted at 254/383 and 151,999,990 expansions. Thus same-seed raw yield rose from v2's 0/3 at 24,512 pairs to v4's 2/3 at 73,536 pairs. The final three-worker survey took 612.47 seconds and reached 297,916 KiB child RSS. The final Tree Compact sweep remained 1/3 at its 48,960-pair allowance; its two incomplete seeds both ended at 254/255 and remain search-incomplete, with no basis for infeasibility.

The predetermined 256 Tree Balanced crossing-forbidden Standard seed 43000 connected 249/255 and exhausted 48,960 pairs after 158,890,240 expansions; its wall time was 132.34 seconds and child RSS 84,276 KiB. Thorough reached 250/255 after 265,813,869 expansions but also exhausted 73,440 pairs in 195.65 seconds. A Standard 65,536-pair probe remained at 249/255 and 159,022,565 expansions; a separate 131,072-pair probe also remained at 249/255 and 159,040,708 expansions after 316.85 seconds. The larger candidate allowance did not improve this seed's Standard result, so the observed failure is not explained by the v4 48,960-pair ceiling alone. The three additional final v4 Standard seeds 43001–43003 also exhausted 48,960 pairs, reaching 248/255, 248/255 and 247/255. Their shared three-worker sweep took 164.25 seconds. For seed 43001, raising the temporary probe limit from 131,072 to 262,144 pairs still reached only 250/255, at 239,697,605 expansions and 646.65 seconds. The extra trials did not improve this result. These bounded attempts do not prove the request infeasible; the crossing-forbidden policy may require a different construction approach. The 131,072-pair probe differs from the retained 65,536-pair source only by its policy label, pair minimum and validator maximum; its two-file overlay archive SHA-256 is `418320d11065a81f91e17dc08b0ed2e973cb1b54af9133caecc73f7bfcd4596b` (tracked patch SHA-256 `70c51ce63983a0b70741eb02d8e67456accfa9972edd6441f620ede31f38a93d`). The 262,144-pair probe applies the analogous tracked overlay to the 131,072-pair probe.

The final 256 Branching Spacious Thorough seed 43001 completed 383/383 connections at 2,785,377,001 expansions, 1,347.64 seconds and 377,588 KiB child RSS. Its 122,844-cell layout confirms that the newly admitted 4,016,046,080-work upper intent can perform useful work well past two billion expansions. Standard retains four production attempts and Thorough eight; raw one-attempt yields do not alone calibrate retry adequacy. Four-attempt `Generate` for crossing-forbidden 256 Tree Balanced seed 43000 still produced no layout: its attempts connected 249, 251, 250 and 238 of 255 and each exhausted 48,960 pair trials, taking 593.86 seconds overall. The existing code reports `generation-exhausted` despite these censored attempts; Plan .18 must relabel that result search-incomplete. In contrast, the hard 256 Branching Compact Standard seed 43002 succeeded on its third of four allowed attempts: the first two stopped at 254/383 after 73,536 pair trials each, and the third completed 383/383 after 41,629 pair trials and 197,151,510 expansions. The whole `Generate` call took 1,051.09 seconds. The 25,857-cell, 614,405-byte output has SHA-256 `e9becca4242eca94c5acf7d68f7a4055e638e7a409e7aaff8b625d6ceddaee5b` and passed independent `Validate`. This demonstrates useful default retry behavior on one censored raw slot, not a guarantee that four retries solve every upper-band request.

## Reproduction and verdict

Build the recorded source in release mode and run, for example:

```sh
cargo build --release -p aic-layout-gen
target/release/aic-layout-gen survey \
  --config docs/benchmarks/layout-generator-scale/configs/256-branching-spacious-fullwork43001-03.json \
  --catalog-dir data/game/normalized --seed 43001 \
  --workspace-budget-gib 16 --output artifacts/layout-generator-scale/branching-spacious
target/release/aic-layout-gen generate \
  --config docs/benchmarks/layout-generator-scale/configs/generate-256-tree-balanced.json \
  --catalog-dir data/game/normalized --seed 43000 --count 1 --workers 1 \
  --workspace-budget-gib 16 --output artifacts/layout-generator-scale/tree-balanced
target/release/aic-layout-gen validate \
  --layout artifacts/layout-generator-scale/tree-balanced/layout-00000.json \
  --catalog-dir data/game/normalized
```

The example `survey` retains rows and metrics but does not publish layouts. The exact survey commands, seeds and deadlines are listed in the benchmark README.

Verdict: **PASS for Plan .22's generator scale-admission and finite-budget repair.** The final v4 policy completed the three formerly work-censored 256-facility Branching Spacious seeds, admitted and used a Thorough request beyond two billion expansions, and recovered a hard Branching Compact seed through the retained four-attempt default. Complete upper-scale Compact/Balanced/Spacious outputs remained below the common 16-MiB publication/read allowance and passed independent validation. The small 100-facility control retained byte-identical output. Sparse placement above the old 250,000-cell ceiling passed its lower-level regression. The reviewer found no missing code path in the declared generator module.

This verdict does not assert general generator yield or training-distribution readiness. Crossing-forbidden Tree Balanced yielded no complete layout in the measured 256-facility Standard/Thorough slots; increased pair trials plateaued, leaving a construction-quality question rather than evidence of infeasibility. The existing `generation-exhausted` code also misclassifies censored attempts; Plan [18](../plans/2026-09-27.18-procedural-generator-route-stop.md) owns that correction. Plan [08](../plans/2026-09-26.08-broad-training-distribution.md) must measure the actual training factory size and pattern distribution offline. Budget exhaustion and deadline interruption describe incomplete searches, never proof that a requested layout is impossible.

## Closeout and cleanup

The generator implementation is commit `2501036c802fc7000b87dae06e3c2a2fd0138a31` on branch `codex/hierarchical-readiness`. The source configs, old/probe source overlays and design addendum are retained in this report's linked files. The v3 intermediate source snapshot was lost before archival; its binary is locally retained, its results have no weight in the adopted-policy verdict, and the exact-source v4 repeat reproduced its substantive outcome. This is the unresolved provenance limitation of the intermediate probe, not of the final policy.

Final verification after the small-DAG addition passed `cargo fmt --all`, `cargo check --workspace`, `cargo test --workspace --quiet`, and `node docs/maintenance/check-tracked-files.mjs --worktree`. The server's final v4 `Survey`, `Generate` and independent `Validate` commands completed as recorded above. The independent code review passed with notes limited to the explicitly open crossing-forbidden and stop-classification follow-ups.

All 78 compact measurement/result and generated-layout files were copied to ignored local `artifacts/plan22-scale-20260927/retained-results.tar.gz` (SHA-256 `a214a87c81c18e30b4ffb2e4e88f67c531ca579d2d7458693e8b69a0b8c7285e`). Its hash matched on both machines and the final v4 reports/layouts were listed before cleanup. The server work tree `/home/ashrimp/Srv/plan22-scale-20260927` was removed after every experiment stopped; the [cleanup manifest](2026-09-27-generator-scale-cleanup.json) records its 1,154,907,502 bytes and free space rising from 161,732,800,512 to 162,941,566,976 bytes. The ignored archive is retained only while downstream validation may need the measured layouts; the committed report, configurations and source patches preserve the durable conclusion.
