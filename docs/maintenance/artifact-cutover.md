# Source-only benchmark artifact cutover

Status: completed and verified, 2026-09-07.
Base: `e31b97a`. This is repository maintenance, not a planner or learning experiment.

## Result and retention rule

Removed 2,246 generated benchmark files from tracking: 251,549,024 bytes, including
raw JSON, duplicate HTML, timing logs, a CPU profile, compressed histories and a
rendered screenshot. Newline counts in those files totaled 4,820,399; this is not a
claim that that many lines of implementation were removed.

The original bytes were moved, not destroyed, from `docs/benchmarks/<path>` to ignored
`artifacts/benchmarks/<path>`. Every moved file's SHA-256 was verified. The ordered
source-path/hash list has digest
`2ebbc755970a0d7b2267bb94ae000d3e9b45a22ed437d1f9076924fa6b7d1b3f`.
A compact local operation record is under `artifacts/maintenance/`. Git history was
not rewritten, so previous commits also retain the old files. This reduces the tracked
checkout and future review noise, not local disk usage by the full moved byte count.
The ignored directory is not a backup or a promise of availability in a fresh clone.

Human findings and decisions remain. Source scripts, reproduction configuration,
hand-reconstructed observations and deliberately selected regression inputs remain.
Four HTML files are authored rendering templates, not generated views, and remain.
Being an input to another experiment does not make a complete raw report source data.
Non-regenerability alone does not justify committing an oversized payload either.

Tracked benchmark content decreased from 255.86 MB to approximately 4.50 MB; its
largest file is now the 62.6 KB graph input. All tracked documentation files are below
100 KiB (largest: the existing 81.4 KB uncommitted-source archival patch).

## Dependencies and what became simpler

- Twelve constructive test modules now use [independent inputs](../benchmarks/fixtures/README.md)
  instead of six historical reports. One 62,593-byte graph preserves all 59 original
  facility IDs and 96 ordered requirements/rational rates. Small coordinate and bank
  variants replace repeated reports; ports and dimensions come from runtime catalogs.
- One small local-order certificate checks independent acceptance/rejection without
  running a research solver. It is not a global factory solution or search history.
- Rust witness-probe tests use a selected 50,110-byte witness-only regression input,
  independent of the JavaScript generator. The 11 KiB interaction regression input
  also remains; neither embeds measurements, rendered output or a candidate history.
- Two tests of fixed historical measured outcomes became explicit input-taking evidence
  audit CLIs. Unit tests no longer require those result snapshots or encode them as
  expectations of general optimizer quality. The audits only check reported fields.
- Active configuration references point directly to external artifact inputs. Documented
  explicit output paths and artifact links were migrated. A tiny rotation-case input
  removes a reproduction command's circular dependency on its own previous summary.
  No runtime compatibility path or automatic artifact fallback was added.

Short artifact names in old reports refer to the corresponding external directory;
source/template names still refer to `docs/benchmarks/`. Reproduction requires the
documented upstream inputs and source version, not just the final command. Timings,
search traces, HTTP observations and interrupted learning streams are not promised
byte-identical on rerun. Recorded historical provenance is not rewritten to describe
current source bytes. To audit the exact old record, use the historical source version
and source overlay with the path layout that its provenance expects.

## Prevention and size limits

The benchmark ignore file excludes all files by default, then permits source/document
extensions and explicit reviewed assets in `benchmark-source-assets.json`. The check
rejects unclassified asset types and force-added outputs. Canonical ignore rules are
verified too: duplicated/missing exceptions and wildcard bypasses are rejected.

Every `docs/` file is limited to 100 KiB, including human reports, patches, handoffs,
manifests and fixtures. Other source files have a 512 KiB limit. Unmodified vendored
game-source tables have a separate finite 4 MiB budget: they are identified upstream
inputs, not experiment records. The existing 2.63 MB `ItemTable.json` is still the
largest such input; changing its upstream-preserving import contract is outside this
benchmark cutover. Normalized runtime catalogs remain external runtime inputs.
Do not evade review through compression, minification, or arbitrary chunking.

`node docs/maintenance/check-tracked-files.mjs --index` checks staged blob sizes,
not merely smaller unstaged replacements. `--worktree` provides an earlier check.
Both are required by `AGENTS.md`; no machine-global Git hook was installed.

## Verification and review

- Workspace check and 526 tests passed, with two existing ignored tests; dev/test debug
  symbols and incremental compilation were
  disabled for this run. This recreated about 711 MiB of build data, not the prior
  13 GiB debug cache. No Cargo profile or solver behavior was changed.
- A source-only checkout of the staged index passed all 419 Node tests, including the
  final negative fixture checks, without any raw experiment directory or uncommitted
  source overlay. Only the two existing release executables were supplied. The earlier
  full working-tree run also passed 426 tests, including the uncommitted breadth tests,
  before the final one-test addition. All six hygiene integration/unit checks passed.
- Both explicit historical evidence-audit CLIs passed on the relocated snapshots;
  a missing-input control returned the expected path-specific diagnostic. Index and
  worktree hygiene checks and `git diff --check` passed.
- Independent review identified missing target identity in the geometry fingerprint
  and duplicate block membership handling; the fixture checks were strengthened.
  The ignore-rule duplicate-exception bug was reproduced locally and fixed. The
  extension-bypass finding was covered by a source-only default plus explicit type checks.
- Existing 12-file uncommitted source overlay and its archived patch remained byte-for-byte
  unchanged. No learning run was resumed; checkpoints and episode data were not modified.

The paused learning handoff's original source hashes remain historical evidence.
Moving its graph input and updating `sampling.json` intentionally means current source
paths/bytes no longer match that frozen source set. Do not rewrite old hashes or call a
new run an uninterrupted continuation. See the updated handoff for restart constraints.
