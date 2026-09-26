# Hierarchical training distribution gap

Status: confirmed local configuration gap; no server access or training run.

## Question and method

Does the final hierarchical learner's configured curriculum expose the production graph sizes and connection patterns that its problem generator can produce? Inspect the accepted design and committed curriculum, then use the existing `prepare-hierarchical-panel` CLI and the same production sampling policy to draw bounded local samples at three size ranges. These samples audit the generator and configuration; they are not model quality evidence.

Source identity: `78c9029f` for the unchanged generator, sampler, CLI and committed curriculum. The in-progress route allowance edits do not change these sampling paths. Sampling policy: `docs/benchmarks/direct-constructive-full-graphs/sampling.json` (maximum 256 facilities). Catalog: `data/game/normalized`. Temporary panels are under ignored `artifacts/plan02-budget-sensitivity/`.

## Reproduction

Run the `aic-cli learning prepare-hierarchical-panel` command with `--sampling docs/benchmarks/direct-constructive-full-graphs/sampling.json --catalog-root data/game/normalized --partition training`, using these bounded arguments:

| Probe | Cases | Seed | Namespace | Facility range | Maximum terminals | Maximum connections |
| --- | ---: | ---: | --- | --- | ---: | ---: |
| Committed curriculum envelope | 32 | 20260926 | `plan02-canonical-probe` | 1–3 | 8 | 8 |
| Intermediate production range | 32 | 20260926 | `plan02-range-probe` | 4–64 | 128 | 256 |
| Upper production range | 16 | 20260927 | `plan02-upper-range-probe` | 65–256 | 1024 | 2048 |

Each command wrote a uniquely named `*-panel.json` in the ignored directory. The CLI reported 4096, 4096 and 2048 attempts respectively. The generated panel schemas and all cases were inspected locally.

## Observed coverage

| Probe | Facilities observed | Connections observed | Cases with pipe edges | Cases with endpoint degree at least 3 |
| --- | ---: | ---: | ---: | ---: |
| Committed curriculum envelope | 1–3 | 2–8 | 18/32 | 25/32 |
| Intermediate production range | 4–64 | 6–160 | 16/32 | 29/32 |
| Upper production range | 80–219 | 176–453 | 8/16 | 16/16 |

The committed `docs/benchmarks/hierarchical-curriculum/curriculum.json` trains levels with exactly 1, 2 and 3 facilities and at most 8 connections. Recent stability and confirmation curricula use exactly 2 facilities and at most 6 connections. Thus they omit every sampled intermediate and upper graph regardless of the generator's broader 256-facility admission ceiling. The official `20 × 20` search bounds also require an explicit scale strategy before large physical layouts can be trained. Pipe and branching do occur in the small range, but their small-graph geometry cannot establish large-layout coverage or route-component diversity.

## Verdict and required gate

The current curriculum is a diagnostic small-graph curriculum, not a sufficiently broad final training distribution. Do not use the small route/crossing panels or the present 1–3-facility configuration as evidence that final model exposure is broad. Before follow-up training is authorized, specify the intended production size envelope, make the training and fixed held-out samplers cover its size bands and transport/topology patterns, verify realized counts and disjoint identities from actual prepared panels, and verify that placement bounds, model input size and request allowances do not truncate the upper bands. Keep a separate small panel for fast regression checks. The correction and its independent review are planned in `docs/plans/2026-09-26.08-broad-training-distribution.md`.
