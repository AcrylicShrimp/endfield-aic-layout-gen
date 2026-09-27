# Upper-scale generator budget calibration

The JSON files in `configs/` are the exact source configurations for the
2026-09-27 generator scale repair. The report with results, code identities,
resource measurements and interpretation is
[`docs/reports/2026-09-27-generator-scale-budget-contract.md`](../../reports/2026-09-27-generator-scale-budget-contract.md).

- Unsuffixed survey files use one attempt, seed `43000`, one worker and a
  300-second deadline. They cover the old resource caps after the invalid
  configuration ceiling was removed.
- `-v2` files repeat the same request and seed with the revised policy and a
  900-second deadline.
- `-seeds43001-03` files use three slots and three workers. Their seeds are
  `43001`, `43002` and `43003`, with a 1,200-second common deadline and an
  explicit 16-GiB conservative aggregate workspace admission.
- `generate-*` files publish one complete layout for seed `43000` with the
  revised policy, except where the report names another seed. The 100-facility
  control was also generated from the old binary to compare output identity.
- `generate-256-branching-compact.json` at seed `43002` and
  `generate-256-tree-balanced-no-crossings.json` at seed `43000` test whether
  the retained Standard four-attempt default recovers hard raw slots.
- `256-tree-spacious-thorough-admission.json` uses a one-second deadline to
  exercise the explicit workspace admission. It is not a yield sample.
- `*-pairprobe*.json` repeats compact tree and branching failures with a
  temporary 65,536-pair-trial probe binary to distinguish budget exhaustion
  from a construction dead end.
- `256-branching-spacious-fullwork43001-03.json` repeats three work-limited
  branching Spacious seeds with the full checked size-dependent route-work
  formula and a 1,800-second common deadline.
- `256-branching-spacious-thorough43001.json` probes one 256-facility upper
  request under Thorough effort with a 3,600-second deadline.
- `256-tree-balanced-no-crossings43000.json` and its `-thorough` counterpart
  hold seed 43000 and the Tree Balanced request fixed while forbidding
  crossings; they use 900- and 1,200-second deadlines respectively.
- `256-tree-balanced-no-crossings-seeds43001-03.json` adds three independent
  Standard upper-band slots to measure whether the crossing-free failure is
  specific to seed 43000; its common deadline is 900 seconds.
- The same single-slot crossing-forbidden source config was also run with seed
  `43001` under temporary 131,072- and 262,144-pair probe binaries. Their
  exact source overlays are linked by the report; the CLI seed, not the source
  filename suffix, selects the second request.

For example, from the repository root after a release build:

```sh
target/release/aic-layout-gen survey \
  --config docs/benchmarks/layout-generator-scale/configs/256-tree-balanced-seeds43001-03.json \
  --catalog-dir data/game/normalized --seed 43001 \
  --workspace-budget-gib 16 --output artifacts/layout-generator-scale/balanced-three
```

These are experimental generator measurements. The original scale-aware
generator design and its prior-art review explain why this generator alone is
not the production random-factory data source.
