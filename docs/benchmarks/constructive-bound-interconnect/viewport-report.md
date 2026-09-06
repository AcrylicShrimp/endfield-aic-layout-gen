# Stable diagnostic viewport

## Failure and repair contract

While inspecting the 41x52 heavy-xiranite incumbent from the lookahead slice, a
focused screenshot unexpectedly repeated the full view. A delayed browser snapshot
confirmed that `view` had reset to `fullView` after `focusLabels` returned true.

The SVG had an intrinsic height derived from its current viewBox aspect ratio.
Focusing the wide, shallow bottom region changed that height. `ResizeObserver(draw)`
then redrew the entire diagram, resetting the camera. An assertion immediately after
an interaction could pass before the resize notification undid it. The confirmed
reproduction is focus; the unconditional observer could reset any camera state when
a resize notification arrived.

The viewport now has an explicit height derived only from the full diagram, capped
at the existing 820 CSS pixels. The observer redraws only when client width changes,
not in response to its own height update. True responsive width changes refit the
diagram; Fit and mode changes also intentionally refit it. Focus, pan and zoom retain
their state across height-only resize notifications. No witness, score, validation,
routing or facility data changes.

## Verification and reproduction

Two new Node tests execute the actual emitted browser script with a small DOM fixture.
They exercise repeated height notifications, focused-view persistence, fixed viewport
height, unchanged witness data, Fit, genuine width changes and the explicit mode reset.
All 267 Node tests, cargo check and 524 Rust tests pass (two existing ignores).

`viewport-browser.mjs` adds a reusable real-browser CLI. Provide an installed Chromium
headless executable, an already rendered local page, comma-separated focus labels and
a new artifact directory:

```sh
node docs/benchmarks/constructive-bound-interconnect/viewport-browser.mjs \
  --browser /absolute/path/to/chrome-headless-shell \
  --page target/shared-results/bound-interconnect-closure-lookahead-viewport.html \
  --focus F48,F37,F49,F39 \
  --output-directory target/viewport-check-new
```

The test waits after focus, wheel zoom and pointer pan, compares persistent camera
state, checks Fit and page overflow, and captures full/focused screenshots. All six
combinations of dark/light and 1400/736/360 width pass in
target/lookahead-viewport-controls/report.json. Running it against the frozen old
page fails on focus, independently reproducing the regression. Browser processes
have a bounded lifetime and are awaited on cleanup.

Personally inspected the full and focused new screenshots. The bottom F48/F37 pair
and its shorter seed return are now inspectable without the camera snapping back.
The renderer's witness hash equals the unchanged final validated 2,132 / 392 / 38
factory. This is a viewer correction, not another layout improvement.

Final native page: http://100.114.24.86:8765/bound-interconnect-closure-lookahead-viewport.html.
No new hosting service or application was created.

The preceding optimization reports record source hashes at commit e67c08d, before
this renderer-only change. Their measured results and witness validation remain
historical evidence. The strict lookahead provenance gate intentionally rejects
those old seed reports against this revision; generate fresh seeds with the current
local runner for a new experiment. Do not rewrite old report hashes to conceal that
source change. The stale-report rejection was manually checked before output creation.
