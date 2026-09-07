# Direct geometry audit of the first streaming controls

Status: inspected; fails useful dense-packing evidence, with valid original factory semantics.

Selection was mechanical, after the first run: largest facility count among validation roots,
and largest trained/untrained retained-area ratio, both at the longest horizon and first
evaluation seed. These are diagnostic outliers, not typical-result or learned-success estimates.
The comparison uses the untrained policy's retained layout, not the loose constructor.

- The 18-facility battery case changes `49 x 54 = 2646` to `45 x 54 = 2430`. The actual drawing
  shows a narrower right side but large interior gaps and long perimeter/corridor routing on
  both sides. Empty XY area remains 62.0% in the trained output (65.3% untrained). Transport
  tiles increase 648 -> 654 and turns 111 -> 115. This is an area improvement, not dense packing.
- The two-facility battery case is the largest relative area regression: `8 x 12 = 96` to
  `8 x 13 = 104`. The trained output visibly separates the lower facility and changes its
  orientation/approaches; transport grows 38 -> 40 and turns 9 -> 12. Empty area rises
  26.0% -> 29.8%. A valid output is not automatically a good learned output.

Every displayed witness passed a fresh geometry check and original request coverage audit.
The renderer checked source episode hashes, original generated-root identity, catalog hashes,
retained score and witness hash. `target/layout-direct-visual-02/report.json` contains all four
audits, density and route diagnostics. Long-looking routes are not claimed redundant without
an obstacle-aware alternative; no filler geometry was added for the view.

The actual dark 1,024px images of both cases were directly inspected. Browser checks passed
all 12 light/dark x 360/736/1,024px x two-case combinations, including case/facility selection,
exact displayed facility counts, metric updates, label size and no horizontal clipping. A common
tile scale and normalized origins make footprint changes visible. The visual follows the inline
visualization skill; it has no network requests or layout mutation controls.

```sh
node docs/benchmarks/layout-direct/visual.mjs \
  --plan docs/benchmarks/layout-direct/sampling.json \
  --report target/layout-direct-train-01/report.json \
  --roots target/layout-direct-train-01/evaluation-roots.json \
  --output-directory target/layout-direct-visual-02
node docs/benchmarks/layout-direct/browser-check.mjs \
  --chrome /path/to/chrome-headless-shell --theme-css /path/to/visualize.css \
  --directory target/layout-direct-visual-02
```

Use new output directories to reproduce without overwriting evidence. Browser inspection briefly
overlapped the subsequent credit-training run; that run's wall times are not isolated benchmarks.
The inspected first run had already completed. No live training source was edited by this visual
slice, and no production architecture or game semantics changed.

The final renderer/browser revision repeats all four fresh witness audits and twelve browser
checks. Its fragment hash matches the directly inspected first render. Direct Node tests and
workspace check/tests pass; no Rust source changed. Existing user changes remain untouched.
