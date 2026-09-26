# Hierarchical distribution quota probe

Status: bounded local sampler finding; final training-distribution readiness remains blocked. No server or GPU training was used.

## Question and source identity

Can the current production graph sampler provide the minimum size and transport diversity proposed for the final hierarchical training and fixed held-out partitions? This probe checks prepared logical graphs only. It does not establish physical layout feasibility, accepted route/repair patterns, usable learning targets, or examples selected by either optimizer.

The generator and `prepare-hierarchical-panel` CLI source are unchanged from `74da757c34207b75fa4f2ba68d132bc9ebd2727b`; unrelated Plan 05 routing/search edits were present in the worktree when the local CLI was built. Sampling policy: `docs/benchmarks/direct-constructive-full-graphs/sampling.json`, SHA-256 `69c9fe638fc23af815c00ca4698d47d6e448fb15616e1235ef3c3e993a627fc6`. Catalog: `data/game/normalized`. The CLI's `held-out` panel partition uses the generator's `validation` partition.

Catalog file SHA-256 identities: `recipes.json` `0fa9f71b6ab2c66ef774ad54f27ddac8682c4ccd6134730d69ca7506836f1ad2`; `items.json` `b49b70d60bee6dc421817e0f6a793532866dfe8618c2e9111c17d032e3fc9164`; `facilities.json` `a314f24f7220297f5d4db65d4612d76c1f7b9c78e16f5c4bb6358b3834a69a9e`; `transports.json` `c3e7c7b324ad175870b0d61830d8df9296dc6a67791fd48d7c4f474a86cee8d1`; `logistics-components.json` `3d7e3e59f1fef8d534c5aab5ba44d60b7ea759e2307c7ad60a7d2cc4def80608`.

## Reproduction

Build locally with `LIBTORCH=$PWD/artifacts/local-libtorch/torch cargo build -p aic-cli --features learning --bin aic-cli`. Run `target/debug/aic-cli learning prepare-hierarchical-panel` with `DYLD_LIBRARY_PATH=$PWD/artifacts/local-libtorch/torch/lib`, `--sampling docs/benchmarks/direct-constructive-full-graphs/sampling.json`, `--catalog-root data/game/normalized`, and the arguments below. Every output path was under ignored `artifacts/plan08-distribution-probe/`. The graph signature check used canonical sorted-key JSON of each `problem`; endpoint degree at least three labels logical branching.

The upper training and seed-20260929 held-out outputs were `upper-training-panel.json` and `upper-heldout-panel.json`; alternate upper outputs were `upper-heldout-SEED-panel.json`; the other outputs were `small-heldout-panel.json` and `medium-heldout-panel.json`. The omitted CLI flags used their defaults: `--max-turns 10000 --max-extension 32`.

| Probe | Partition | Cases | Seed | Namespace | Facilities | Max terminals | Max connections |
| --- | --- | ---: | ---: | --- | --- | ---: | ---: |
| Upper training | `training` | 32 | 20260928 | `plan08-upper-local-training` | 65–256 | 1024 | 2048 |
| Upper held-out | `held-out` | 8 | 20260929 | `plan08-upper-local-heldout` | 65–256 | 1024 | 2048 |
| Upper held-out alternates | `held-out` | 8 each | 20260930–20260933 | `plan08-upper-heldout-SEED` | 65–256 | 1024 | 2048 |
| Small held-out | `held-out` | 8 | 20260929 | `plan08-small-local-heldout` | 1–3 | 8 | 8 |
| Medium held-out | `held-out` | 8 | 20260929 | `plan08-medium-local-heldout` | 4–64 | 128 | 256 |

The upper held-out request for 16 cases at seed 20260929 used namespace `plan08-upper-local-heldout16`; 32 cases used `plan08-upper-local-heldout32`. Both used the same upper bounds. The sampler derives its attempt limit as `max(requested_cases, 16) × 128`, so 8- and 16-case requests each had 2,048 attempts and 32 cases had 4,096.

## Results

| Probe | Accepted candidates / attempts | Facility range | Terminal range | Connection range | Belt-only | Pipe-containing | Logical branching |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Upper training, 32 selected | 58 / 4096 | 71–240 | 16–336 | 128–512 | 19 | 13 | 32 |
| Upper held-out, seed 20260929, 8 selected | 8 / 2048 | 72–164 | 48–158 | 160–358 | 0 | 8 | 8 |
| Upper held-out, seeds 20260930–33, 8 selected each | 8, 8, 11, 8 / 2048 each | 74–256 pooled | 15–368 pooled | 124–592 pooled | 1, 2, 1, 2 | 7, 6, 7, 6 | 8 each |
| Small held-out, 8 selected | 160 / 2048 | 1–3 | 2–6 | 2–7 | 2 | 6 | 7 |
| Medium held-out, 8 selected | 243 / 2048 | 4–60 | 4–100 | 6–152 | 3 | 5 | 7 |

All 32 selected upper training graphs and all 8 selected upper held-out graphs had distinct canonical problem identities; the two selected sets had zero identity overlap. The upper held-out request for 16 cases failed with only 8 unique accepted graphs after 2,048 attempts. The request for 32 failed with only 17 after 4,096 attempts. These failures are evidence of the current attempt budget and validation-partition acceptance rate, not proof that additional graphs or belt-only graphs do not exist.

The selected small, medium and upper held-out panels all missed the proposed minimum of four belt-only graphs per band. The upper band also had too few accepted candidates for a quota-aware selector to reliably choose a balanced eight-case subset. Increasing requested output count solely to obtain more attempts is not a valid fix: it changes the requested sample and failed in the two upper probes. A separately configurable, finite candidate-generation effort budget and quota-aware selection from the accepted pool are required. Calibrate that budget on more than one fixed seed; if a required stratum remains unavailable, publish a named blocked diagnostic with attempt and accepted-pool counts rather than silently changing the quota or reducing the graph size.

This is a sampler/input finding. No large physical case was placed or routed, and neither model received an optimizer update. The full Plan 08 gate still needs actual route/component/crossing/repair targets and size × pattern selected-example counts for both models.

## Immediate physical bound obstruction

The committed hierarchical curriculum search configuration has inclusive bounds `[0,20] × [0,20]`, an area of `21 × 21 = 441` cells (`Bounds::width/height` add one to the coordinate difference). The learning placement domain requires every rotated facility footprint to fit inside those bounds, and native placement rejects overlapping body cells. Every facility in the current normalized catalog has a footprint of at least 9 cells. Thus even the 65-facility minimum of the intended upper band needs at least `65 × 9 = 585` nonoverlapping facility cells, before terminals, routes, crossings or spacing. No upper-band case can fit the current curriculum search bounds. This arithmetic is a necessary area bound, not a proof that enlarging the bounds will make those graphs physically routable. Plan 08 must bind and measure an upper-band physical domain before treating its logical graph samples as usable targets.

Independent read-only audit `/root/review_distribution_plan_caps`: PASS. It reproduced the two failed 16/32-case requests and independently matched all eight panel counts, graph-identity overlap, sampling/catalog hashes and narrow interpretation. No server was used.

## Cleanup

After the independent audit matched the panels and this compact report was checked, the eight regenerable temporary JSON files were removed: `medium-heldout-panel.json`, `small-heldout-panel.json`, `upper-heldout-panel.json`, `upper-heldout-20260930-panel.json`, `upper-heldout-20260931-panel.json`, `upper-heldout-20260932-panel.json`, `upper-heldout-20260933-panel.json`, and `upper-training-panel.json`, all under `artifacts/plan08-distribution-probe/`. The directory changed from 7,896 KiB to 0 KiB by `du -sk`. No checkpoint or non-replayable evidence was removed; exact seeds, source/config identities, commands, metrics and the blocked verdict remain above.
